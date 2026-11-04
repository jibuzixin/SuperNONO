//! Premultiplied RGBA presentation into a transparent window.

use winit::window::Window;

#[derive(Debug, thiserror::Error)]
pub enum PresenterError {
    #[error("transparent presentation is not implemented on this platform yet")]
    Unsupported,
    #[error("window does not expose an AppKit view")]
    NoAppKitView,
    #[error("window view is not layer-backed")]
    NoLayer,
    #[error("CoreGraphics rejected the frame")]
    ImageRejected,
}

/// Uploads a rendered frame into the window's layer.
///
/// `rgba` is premultiplied RGBA8 (`width * height * 4` bytes) and `dpi` is the
/// monitor scale factor; the caller renders at `dpi` so the vector art stays
/// crisp on Retina displays.
pub trait Presenter {
    fn present(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
        dpi: f64,
    ) -> Result<(), PresenterError>;

    fn resize(&mut self, width: u32, height: u32, dpi: f64) -> Result<(), PresenterError>;
}

pub fn create_presenter(window: &Window) -> Result<Box<dyn Presenter>, PresenterError> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacPresenter::new(window)?))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Err(PresenterError::Unsupported)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSView;
    use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
    use objc2_core_graphics::{
        CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
        CGImageByteOrderInfo, CGImageComponentInfo, CGImagePixelFormatInfo,
    };
    use objc2_quartz_core::{kCAGravityTopLeft, CALayer, CATransaction};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use super::{Presenter, PresenterError};

    /// Frees the pixel copy once CoreGraphics is done with the image.
    unsafe extern "C-unwind" fn release_frame_data(
        _info: *mut c_void,
        data: NonNull<c_void>,
        size: usize,
    ) {
        let slice = std::ptr::slice_from_raw_parts_mut(data.as_ptr() as *mut u8, size);
        drop(unsafe { Box::from_raw(slice) });
    }

    pub struct MacPresenter {
        layer: Retained<CALayer>,
        color_space: CFRetained<CGColorSpace>,
        dpi: f64,
        /// Pixel size of the current frame; the layer frame must match it or
        /// larger trail stages would be drawn at the old size.
        width: u32,
        height: u32,
    }

    impl std::fmt::Debug for MacPresenter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MacPresenter").field("dpi", &self.dpi).finish()
        }
    }

    impl MacPresenter {
        pub fn new(window: &Window) -> Result<Self, PresenterError> {
            if MainThreadMarker::new().is_none() {
                return Err(PresenterError::Unsupported);
            }
            let handle = window
                .window_handle()
                .map_err(|_| PresenterError::NoAppKitView)?;
            let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
                return Err(PresenterError::NoAppKitView);
            };
            // SAFETY: the handle is valid for the lifetime of `window` and points
            // at an `NSView` (documented by raw-window-handle).
            let view = unsafe { &*(appkit.ns_view.as_ptr() as *const NSView) };
            view.setWantsLayer(true);
            let root = view.layer().ok_or(PresenterError::NoLayer)?;

            let layer = CALayer::new();
            layer.setAnchorPoint(CGPoint::new(0.0, 0.0));
            layer.setGeometryFlipped(true);
            // SAFETY: `kCAGravityTopLeft` is an immutable framework string.
            layer.setContentsGravity(unsafe { kCAGravityTopLeft });
            layer.setOpaque(false);
            layer.setMasksToBounds(false);

            let color_space = CGColorSpace::new_device_rgb().ok_or(PresenterError::ImageRejected)?;

            let mut presenter = Self {
                layer,
                color_space,
                dpi: window.scale_factor(),
                width: 0,
                height: 0,
            };
            let size = window.inner_size();
            presenter.resize(size.width, size.height, window.scale_factor())?;
            root.addSublayer(&presenter.layer);
            Ok(presenter)
        }
    }

    impl Presenter for MacPresenter {
        fn present(
            &mut self,
            rgba: &[u8],
            width: u32,
            height: u32,
            dpi: f64,
        ) -> Result<(), PresenterError> {
            if width == 0 || height == 0 {
                return Err(PresenterError::ImageRejected);
            }
            if (self.dpi - dpi).abs() > f64::EPSILON
                || self.width != width
                || self.height != height
            {
                self.resize(width, height, dpi)?;
            }

            // CoreGraphics keeps the provider data alive asynchronously, so the
            // frame is moved into its own allocation and released by the callback.
            let boxed = rgba.to_vec().into_boxed_slice();
            let len = boxed.len();
            let data = Box::into_raw(boxed) as *mut u8;

            // SAFETY: `data` is a valid allocation of `len` bytes that the
            // release callback reclaims exactly once.
            let provider = unsafe {
                CGDataProvider::with_data(
                    std::ptr::null_mut(),
                    data as *const c_void,
                    len,
                    Some(release_frame_data),
                )
            }
            .ok_or(PresenterError::ImageRejected)?;

            let bitmap_info = CGBitmapInfo(
                CGImageAlphaInfo::PremultipliedLast.0
                    | CGImageComponentInfo::Integer.0
                    | CGImageByteOrderInfo::Order32Big.0
                    | CGImagePixelFormatInfo::Packed.0,
            );
            // SAFETY: dimensions and stride match the buffer; the colour space
            // and provider outlive the image creation call (the image retains both).
            let image = unsafe {
                CGImage::new(
                    width as usize,
                    height as usize,
                    8,
                    32,
                    width as usize * 4,
                    Some(&self.color_space),
                    bitmap_info,
                    Some(&provider),
                    std::ptr::null(),
                    false,
                    CGColorRenderingIntent::RenderingIntentDefault,
                )
            }
            .ok_or(PresenterError::ImageRejected)?;

            // SAFETY: `image` is a valid CGImage; the layer retains its contents.
            CATransaction::begin();
            CATransaction::setDisableActions(true);
            unsafe { self.layer.setContents(Some(image.as_ref())) };
            CATransaction::commit();
            Ok(())
        }

        fn resize(&mut self, width: u32, height: u32, dpi: f64) -> Result<(), PresenterError> {
            if dpi <= 0.0 {
                return Ok(());
            }
            self.dpi = dpi;
            self.width = width;
            self.height = height;
            self.layer.setContentsScale(dpi);
            let points_w = width as f64 / dpi;
            let points_h = height as f64 / dpi;
            self.layer.setFrame(CGRect::new(
                CGPoint::new(0.0, 0.0),
                CGSize::new(points_w, points_h),
            ));
            Ok(())
        }
    }
}
