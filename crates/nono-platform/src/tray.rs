//! Tray icon and native menu.

use nono_core::{Catalog, Config};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub const MENU_HIDE: &str = "hide";
pub const MENU_QUIT: &str = "quit";
pub const MENU_SOUND: &str = "sound";
pub const MENU_WANDER: &str = "wander";
pub const MENU_IDLE_ANIMATION: &str = "idle_animation";
pub const MENU_ACTION_PREFIX: &str = "action:";
pub const MENU_EXPRESSION_PREFIX: &str = "expression:";
pub const MENU_FORM_PREFIX: &str = "form:";
pub const MENU_SCALE_PREFIX: &str = "scale:";

pub const SCALE_CHOICES: [f32; 5] = [0.8, 1.0, 1.25, 1.5, 2.0];

#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    #[error("menu: {0}")]
    Menu(#[from] tray_icon::menu::Error),
    #[error("tray: {0}")]
    Tray(#[from] tray_icon::Error),
}

/// Owns the tray icon and every menu handle the application later updates.
pub struct TrayMenu {
    pub tray: TrayIcon,
    /// Item text is flipped between "hide" and "show" by the app.
    pub hide: MenuItem,
    pub sound: CheckMenuItem,
    pub wander: CheckMenuItem,
    pub idle_animation: CheckMenuItem,
    pub form_items: Vec<(u8, CheckMenuItem)>,
    pub scale_items: Vec<(f32, CheckMenuItem)>,
    // Keep the owned wrappers alive for as long as the tray lives.
    #[allow(dead_code)]
    keep_items: Vec<MenuItem>,
    #[allow(dead_code)]
    keep_submenus: Vec<Submenu>,
}

impl TrayMenu {
    pub fn sync_checks(&self, config: &Config) {
        self.sound.set_checked(config.sound);
        self.wander.set_checked(config.wander);
        self.idle_animation.set_checked(config.idle_animation);
        for (form, item) in &self.form_items {
            item.set_checked(*form == config.form);
        }
        for (scale, item) in &self.scale_items {
            item.set_checked((*scale - config.scale).abs() < 0.001);
        }
    }
}

/// Builds the tray icon and menu.  `on_menu` receives the raw menu id string.
pub fn build_tray(
    catalog: &Catalog,
    config: &Config,
    icon: Icon,
    on_menu: impl Fn(String) + Send + Sync + 'static,
) -> Result<TrayMenu, TrayError> {
    tray_icon::menu::MenuEvent::set_event_handler(Some(move |event: tray_icon::menu::MenuEvent| {
        on_menu(event.id.0);
    }));

    let menu = Menu::new();
    let mut keep_items = Vec::new();
    let mut keep_submenus = Vec::new();

    // ── actions, grouped by form ─────────────────────────────────────
    let actions_root = Submenu::new("动作", true);
    let expressions_root = Submenu::new("表情", true);
    for form in catalog.forms() {
        let action_sub = Submenu::new(&form.name, true);
        for action in &form.actions {
            let id = format!("{MENU_ACTION_PREFIX}{}:{}", form.id, action.slug);
            let item = MenuItem::with_id(id, &action.name, true, None);
            action_sub.append(&item)?;
            keep_items.push(item);
        }
        if !form.actions.is_empty() {
            actions_root.append(&action_sub)?;
        }

        let expr_sub = Submenu::new(&form.name, true);
        for expr in &form.expressions {
            let id = format!("{MENU_EXPRESSION_PREFIX}{}:{}", form.id, expr.slug);
            let item = MenuItem::with_id(id, &expr.name, true, None);
            expr_sub.append(&item)?;
            keep_items.push(item);
        }
        if !form.expressions.is_empty() {
            expressions_root.append(&expr_sub)?;
        }

        keep_submenus.push(action_sub);
        keep_submenus.push(expr_sub);
    }
    menu.append(&actions_root)?;
    menu.append(&expressions_root)?;
    menu.append(&PredefinedMenuItem::separator())?;

    // ── appearance ───────────────────────────────────────────────────
    let forms_root = Submenu::new("形态", true);
    let mut form_items = Vec::new();
    for form in catalog.forms() {
        let item = CheckMenuItem::with_id(
            format!("{MENU_FORM_PREFIX}{}", form.id),
            &form.name,
            true,
            form.id == config.form,
            None,
        );
        forms_root.append(&item)?;
        form_items.push((form.id, item));
    }
    menu.append(&forms_root)?;
    keep_submenus.push(forms_root);

    let scales_root = Submenu::new("大小", true);
    let mut scale_items = Vec::new();
    for scale in SCALE_CHOICES {
        let item = CheckMenuItem::with_id(
            format!("{MENU_SCALE_PREFIX}{scale}"),
            format!("{:.0}%", scale * 100.0),
            true,
            (scale - config.scale).abs() < 0.001,
            None,
        );
        scales_root.append(&item)?;
        scale_items.push((scale, item));
    }
    menu.append(&scales_root)?;
    keep_submenus.push(scales_root);

    let sound = CheckMenuItem::with_id(MENU_SOUND, "音效", true, config.sound, None);
    let wander = CheckMenuItem::with_id(MENU_WANDER, "自由走动", true, config.wander, None);
    let idle_animation = CheckMenuItem::with_id(
        MENU_IDLE_ANIMATION,
        "静止时播放走路",
        true,
        config.idle_animation,
        None,
    );
    menu.append(&sound)?;
    menu.append(&wander)?;
    menu.append(&idle_animation)?;
    menu.append(&PredefinedMenuItem::separator())?;

    // ── window ───────────────────────────────────────────────────────
    let hide = MenuItem::with_id(MENU_HIDE, "隐藏 NONO", true, None);
    let quit = MenuItem::with_id(MENU_QUIT, "退出 NONO", true, None);
    menu.append(&hide)?;
    menu.append(&quit)?;
    keep_items.push(quit);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_tooltip("超能 NONO")
        .build()?;

    keep_submenus.push(actions_root);
    keep_submenus.push(expressions_root);

    Ok(TrayMenu {
        tray,
        hide: hide.clone(),
        sound,
        wander,
        idle_animation,
        form_items,
        scale_items,
        keep_items,
        keep_submenus,
    })
}
