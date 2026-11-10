//! Tray icon and native menu.

use nono_core::trail;
use nono_core::{speed, Catalog, Config, Display};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub const MENU_HIDE: &str = "hide";
pub const MENU_QUIT: &str = "quit";
pub const MENU_SOUND: &str = "sound";
pub const MENU_WANDER: &str = "wander";
pub const MENU_IDLE_ANIMATION: &str = "idle_animation";
pub const MENU_FOLLOW: &str = "follow";
pub const MENU_SUMMON: &str = "summon";
pub const MENU_SUMMON_ANIMATION: &str = "summon_animation";
pub const MENU_START_AT_CENTER: &str = "start_at_center";
pub const MENU_DISPLAY_PREFIX: &str = "display:";
pub const MENU_DISPLAY_REFRESH: &str = "display_refresh";
pub const MENU_TRAIL_ENABLED: &str = "trail_enabled";
pub const MENU_TRAIL_PREVIEW: &str = "trail_preview";
pub const MENU_TRAIL_INTERVAL_DOWN: &str = "trail_interval_down";
pub const MENU_TRAIL_INTERVAL_UP: &str = "trail_interval_up";
pub const MENU_TRAIL_LIFETIME_DOWN: &str = "trail_lifetime_down";
pub const MENU_TRAIL_LIFETIME_UP: &str = "trail_lifetime_up";
pub const MENU_TRAIL_OPACITY_DOWN: &str = "trail_opacity_down";
pub const MENU_TRAIL_OPACITY_UP: &str = "trail_opacity_up";
pub const MENU_TRAIL_RESET: &str = "trail_reset";
pub const MENU_TRAIL_RELOAD: &str = "trail_reload";
pub const MENU_TRAIL_OPEN_CONFIG: &str = "trail_open_config";
pub const MENU_TRAIL_WATCH: &str = "trail_watch";
pub const MENU_GLOW: &str = "glow";
pub const MENU_SPEED_MOVE_DOWN: &str = "speed_move_down";
pub const MENU_SPEED_MOVE_UP: &str = "speed_move_up";
pub const MENU_SPEED_FOLLOW_DOWN: &str = "speed_follow_down";
pub const MENU_SPEED_FOLLOW_UP: &str = "speed_follow_up";
pub const MENU_SPEED_FOLLOW_GLOBAL: &str = "speed_follow_global";
pub const MENU_SPEED_IDLE_DOWN: &str = "speed_idle_down";
pub const MENU_SPEED_IDLE_UP: &str = "speed_idle_up";
pub const MENU_SPEED_IDLE_GLOBAL: &str = "speed_idle_global";
pub const MENU_SPEED_RESET: &str = "speed_reset";
pub const MENU_TINT_PREFIX: &str = "tint:";
pub const MENU_TINT_PICKER: &str = "tint_picker";
pub const MENU_TINT_HEX: &str = "tint_hex";
pub const MENU_ACTION_PREFIX: &str = "action:";
pub const MENU_EXPRESSION_PREFIX: &str = "expression:";
pub const MENU_FORM_PREFIX: &str = "form:";
pub const MENU_SCALE_PREFIX: &str = "scale:";

pub const SCALE_CHOICES: [f32; 5] = [0.8, 1.0, 1.25, 1.5, 2.0];

/// Body colours offered in the tray.  `None` is the original white.
pub const TINT_PRESETS: [(&str, Option<[u8; 3]>); 8] = [
    ("默认（白）", None),
    ("樱花粉", Some([255, 150, 180])),
    ("柠檬黄", Some([250, 210, 90])),
    ("薄荷绿", Some([120, 220, 160])),
    ("天空蓝", Some([110, 180, 255])),
    ("葡萄紫", Some([190, 150, 255])),
    ("蜜橙", Some([255, 170, 90])),
    ("珊瑚红", Some([255, 120, 110])),
];

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
    /// Runtime glow rebuilt from the assets' own filter parameters.
    pub glow: CheckMenuItem,
    pub wander: CheckMenuItem,
    pub idle_animation: CheckMenuItem,
    /// Runtime-only toggle: follows the mouse for 20 seconds.
    pub follow: CheckMenuItem,
    /// Label follows the form's summon action availability.
    pub summon: MenuItem,
    pub summon_animation: CheckMenuItem,
    pub start_at_center: CheckMenuItem,
    pub display_items: Vec<(u32, CheckMenuItem)>,
    pub form_items: Vec<(u8, CheckMenuItem)>,
    pub scale_items: Vec<(f32, CheckMenuItem)>,
    pub tint_items: Vec<(Option<[u8; 3]>, CheckMenuItem)>,
    pub tint_custom: CheckMenuItem,
    pub tint_hex: MenuItem,
    pub speed_move_value: MenuItem,
    pub speed_follow_value: MenuItem,
    pub speed_idle_value: MenuItem,
    pub speed_follow_global: CheckMenuItem,
    pub speed_idle_global: CheckMenuItem,
    pub trail_enabled: CheckMenuItem,
    /// Read-only rows that show the exact current values.
    pub trail_interval_value: MenuItem,
    pub trail_lifetime_value: MenuItem,
    pub trail_opacity_value: MenuItem,
    /// Runtime-only debugging aid: reload the trail fields when the config file
    /// changes on disk.
    pub trail_watch: CheckMenuItem,
    // Keep the owned wrappers alive for as long as the tray lives.
    #[allow(dead_code)]
    keep_items: Vec<MenuItem>,
    #[allow(dead_code)]
    keep_submenus: Vec<Submenu>,
}

impl TrayMenu {
    pub fn sync_checks(&self, config: &Config) {
        self.sound.set_checked(config.sound);
        self.glow.set_checked(config.glow);
        self.wander.set_checked(config.wander);
        self.idle_animation.set_checked(config.idle_animation);
        for (form, item) in &self.form_items {
            item.set_checked(*form == config.form);
        }
        for (scale, item) in &self.scale_items {
            item.set_checked((*scale - config.scale).abs() < 0.001);
        }
        for (value, item) in &self.tint_items {
            item.set_checked(*value == config.tint);
        }
        let is_preset = TINT_PRESETS.iter().any(|(_, value)| *value == config.tint);
        let custom = config.tint.filter(|_| !is_preset);
        self.tint_custom.set_checked(custom.is_some());
        let custom_text = match custom {
            Some([r, g, b]) => format!("自定义颜色（#{r:02X}{g:02X}{b:02X}）…"),
            None => "自定义颜色（色盘）…".to_string(),
        };
        self.tint_custom.set_text(&custom_text);
        self.speed_move_value
            .set_text(format!("全局：{:.0} pt/s", config.move_speed));
        let follow = config.effective_follow_speed();
        self.speed_follow_value
            .set_text(if config.follow_speed.is_none() {
                format!("跟随：跟随全局（{:.0} pt/s）", follow)
            } else {
                format!("跟随：{:.0} pt/s", follow)
            });
        self.speed_follow_global
            .set_checked(config.follow_speed.is_none());
        let idle = config.effective_idle_speed();
        self.speed_idle_value
            .set_text(if config.idle_speed.is_none() {
                format!("自由移动：跟随全局（{:.0} pt/s）", idle)
            } else {
                format!("自由移动：{:.0} pt/s", idle)
            });
        self.speed_idle_global
            .set_checked(config.idle_speed.is_none());
        self.trail_enabled.set_checked(config.trail_enabled);
        self.trail_interval_value
            .set_text(format!("间隔：{} 毫秒", config.trail_interval_ms));
        self.trail_lifetime_value
            .set_text(format!("停留：{} 毫秒", config.trail_lifetime_ms));
        self.trail_opacity_value
            .set_text(format!("浓度：{:.0}%", config.trail_opacity * 100.0));
        self.summon_animation.set_checked(config.summon_animation);
        self.start_at_center.set_checked(config.start_at_center);
    }

    /// Reflects the current allowed-display selection in the tray.
    pub fn sync_displays(&self, selected: &[u32]) {
        for (id, item) in &self.display_items {
            item.set_checked(selected.contains(id));
        }
    }
}

fn display_label(display: &Display) -> String {
    let role = if display.primary {
        "主显示器"
    } else {
        "显示器"
    };
    if display.name.trim().is_empty() {
        format!("{role}（{}）", display.size_label())
    } else {
        format!("{role} · {}（{}）", display.name, display.size_label())
    }
}

/// Builds the tray icon and menu.  `on_menu` receives the raw menu id string.
pub fn build_tray(
    catalog: &Catalog,
    config: &Config,
    displays: &[Display],
    selected_displays: &[u32],
    icon: Icon,
    on_menu: impl Fn(String) + Send + Sync + 'static,
) -> Result<TrayMenu, TrayError> {
    tray_icon::menu::MenuEvent::set_event_handler(Some(
        move |event: tray_icon::menu::MenuEvent| {
            on_menu(event.id.0);
        },
    ));

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

    let tints_root = Submenu::new("颜色", true);
    let mut tint_items = Vec::new();
    for (name, value) in TINT_PRESETS {
        let id = match value {
            None => format!("{MENU_TINT_PREFIX}default"),
            Some(c) => format!("{MENU_TINT_PREFIX}{},{},{}", c[0], c[1], c[2]),
        };
        let item = CheckMenuItem::with_id(id, name, true, value == config.tint, None);
        tints_root.append(&item)?;
        tint_items.push((value, item));
    }
    tints_root.append(&PredefinedMenuItem::separator())?;
    let is_preset = TINT_PRESETS.iter().any(|(_, value)| *value == config.tint);
    let custom = config.tint.filter(|_| !is_preset);
    let custom_text = match custom {
        Some([r, g, b]) => format!("自定义颜色（#{r:02X}{g:02X}{b:02X}）…"),
        None => "自定义颜色（色盘）…".to_string(),
    };
    let tint_custom =
        CheckMenuItem::with_id(MENU_TINT_PICKER, &custom_text, true, custom.is_some(), None);
    let tint_hex = MenuItem::with_id(MENU_TINT_HEX, "输入十六进制色值…", true, None);
    let tint_hint = MenuItem::new("色盘关闭后应用", false, None);
    tints_root.append(&tint_custom)?;
    tints_root.append(&tint_hex)?;
    tints_root.append(&tint_hint)?;
    keep_items.push(tint_hex.clone());
    keep_items.push(tint_hint);
    menu.append(&tints_root)?;
    keep_submenus.push(tints_root);

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

    // ── which displays the pet may roam on ───────────────────────────
    let displays_root = Submenu::new("移动范围", true);
    let mut display_items = Vec::new();
    for display in displays {
        let item = CheckMenuItem::with_id(
            format!("{MENU_DISPLAY_PREFIX}{}", display.id),
            display_label(display),
            true,
            selected_displays.contains(&display.id),
            None,
        );
        displays_root.append(&item)?;
        display_items.push((display.id, item));
    }
    if !displays.is_empty() {
        displays_root.append(&PredefinedMenuItem::separator())?;
    }
    let display_refresh = MenuItem::with_id(MENU_DISPLAY_REFRESH, "刷新显示器列表", true, None);
    displays_root.append(&display_refresh)?;
    keep_items.push(display_refresh);
    menu.append(&displays_root)?;
    keep_submenus.push(displays_root);

    let speed_root = Submenu::new("速度", true);
    let speed_move_value =
        MenuItem::new(format!("全局：{:.0} pt/s", config.move_speed), false, None);
    let move_down = MenuItem::with_id(
        MENU_SPEED_MOVE_DOWN,
        format!("−{:.0} pt/s", speed::STEP),
        true,
        None,
    );
    let move_up = MenuItem::with_id(
        MENU_SPEED_MOVE_UP,
        format!("+{:.0} pt/s", speed::STEP),
        true,
        None,
    );
    speed_root.append(&speed_move_value)?;
    speed_root.append(&move_down)?;
    speed_root.append(&move_up)?;
    speed_root.append(&PredefinedMenuItem::separator())?;

    let speed_follow_value = MenuItem::new(
        format!("跟随：{:.0} pt/s", config.effective_follow_speed()),
        false,
        None,
    );
    let follow_down = MenuItem::with_id(
        MENU_SPEED_FOLLOW_DOWN,
        format!("−{:.0} pt/s", speed::STEP),
        true,
        None,
    );
    let follow_up = MenuItem::with_id(
        MENU_SPEED_FOLLOW_UP,
        format!("+{:.0} pt/s", speed::STEP),
        true,
        None,
    );
    let speed_follow_global = CheckMenuItem::with_id(
        MENU_SPEED_FOLLOW_GLOBAL,
        "跟随使用全局速度",
        true,
        config.follow_speed.is_none(),
        None,
    );
    speed_root.append(&speed_follow_value)?;
    speed_root.append(&follow_down)?;
    speed_root.append(&follow_up)?;
    speed_root.append(&speed_follow_global)?;
    speed_root.append(&PredefinedMenuItem::separator())?;

    let speed_idle_value = MenuItem::new(
        format!("自由移动：{:.0} pt/s", config.effective_idle_speed()),
        false,
        None,
    );
    let idle_down = MenuItem::with_id(
        MENU_SPEED_IDLE_DOWN,
        format!("−{:.0} pt/s", speed::STEP),
        true,
        None,
    );
    let idle_up = MenuItem::with_id(
        MENU_SPEED_IDLE_UP,
        format!("+{:.0} pt/s", speed::STEP),
        true,
        None,
    );
    let speed_idle_global = CheckMenuItem::with_id(
        MENU_SPEED_IDLE_GLOBAL,
        "自由移动使用全局速度",
        true,
        config.idle_speed.is_none(),
        None,
    );
    speed_root.append(&speed_idle_value)?;
    speed_root.append(&idle_down)?;
    speed_root.append(&idle_up)?;
    speed_root.append(&speed_idle_global)?;
    speed_root.append(&PredefinedMenuItem::separator())?;

    let speed_reset = MenuItem::with_id(MENU_SPEED_RESET, "恢复默认速度", true, None);
    speed_root.append(&speed_reset)?;

    keep_items.push(move_down);
    keep_items.push(move_up);
    keep_items.push(follow_down);
    keep_items.push(follow_up);
    keep_items.push(idle_down);
    keep_items.push(idle_up);
    keep_items.push(speed_reset);

    menu.append(&speed_root)?;
    keep_submenus.push(speed_root);

    let trail_root = Submenu::new("残影", true);
    let trail_enabled =
        CheckMenuItem::with_id(MENU_TRAIL_ENABLED, "开启", true, config.trail_enabled, None);
    let trail_preview = MenuItem::with_id(MENU_TRAIL_PREVIEW, "预览一次（走一小段）", true, None);
    trail_root.append(&trail_enabled)?;
    trail_root.append(&trail_preview)?;
    trail_root.append(&PredefinedMenuItem::separator())?;

    let trail_interval_value = MenuItem::new(
        format!("间隔：{} 毫秒", config.trail_interval_ms),
        false,
        None,
    );
    let interval_down = MenuItem::with_id(
        MENU_TRAIL_INTERVAL_DOWN,
        format!("−{} 毫秒（更密）", trail::INTERVAL_STEP_MS),
        true,
        None,
    );
    let interval_up = MenuItem::with_id(
        MENU_TRAIL_INTERVAL_UP,
        format!("+{} 毫秒（更疏）", trail::INTERVAL_STEP_MS),
        true,
        None,
    );
    trail_root.append(&trail_interval_value)?;
    trail_root.append(&interval_down)?;
    trail_root.append(&interval_up)?;
    trail_root.append(&PredefinedMenuItem::separator())?;

    let trail_lifetime_value = MenuItem::new(
        format!("停留：{} 毫秒", config.trail_lifetime_ms),
        false,
        None,
    );
    let lifetime_down = MenuItem::with_id(
        MENU_TRAIL_LIFETIME_DOWN,
        format!("−{} 毫秒（更短）", trail::LIFETIME_STEP_MS),
        true,
        None,
    );
    let lifetime_up = MenuItem::with_id(
        MENU_TRAIL_LIFETIME_UP,
        format!("+{} 毫秒（更长）", trail::LIFETIME_STEP_MS),
        true,
        None,
    );
    trail_root.append(&trail_lifetime_value)?;
    trail_root.append(&lifetime_down)?;
    trail_root.append(&lifetime_up)?;
    trail_root.append(&PredefinedMenuItem::separator())?;

    let trail_opacity_value = MenuItem::new(
        format!("浓度：{:.0}%", config.trail_opacity * 100.0),
        false,
        None,
    );
    let opacity_down = MenuItem::with_id(
        MENU_TRAIL_OPACITY_DOWN,
        format!("−{:.0}%（更淡）", trail::OPACITY_STEP * 100.0),
        true,
        None,
    );
    let opacity_up = MenuItem::with_id(
        MENU_TRAIL_OPACITY_UP,
        format!("+{:.0}%（更浓）", trail::OPACITY_STEP * 100.0),
        true,
        None,
    );
    trail_root.append(&trail_opacity_value)?;
    trail_root.append(&opacity_down)?;
    trail_root.append(&opacity_up)?;
    trail_root.append(&PredefinedMenuItem::separator())?;

    let trail_reset = MenuItem::with_id(MENU_TRAIL_RESET, "恢复默认", true, None);
    let trail_watch =
        CheckMenuItem::with_id(MENU_TRAIL_WATCH, "自动重载配置（调试）", true, false, None);
    let trail_reload = MenuItem::with_id(MENU_TRAIL_RELOAD, "重新载入残影/速度设置", true, None);
    let trail_open_config = MenuItem::with_id(MENU_TRAIL_OPEN_CONFIG, "打开配置文件…", true, None);
    trail_root.append(&trail_reset)?;
    trail_root.append(&trail_watch)?;
    trail_root.append(&trail_reload)?;
    trail_root.append(&trail_open_config)?;

    keep_items.push(interval_down);
    keep_items.push(interval_up);
    keep_items.push(lifetime_down);
    keep_items.push(lifetime_up);
    keep_items.push(opacity_down);
    keep_items.push(opacity_up);
    keep_items.push(trail_preview);
    keep_items.push(trail_reset);
    keep_items.push(trail_reload);
    keep_items.push(trail_open_config);

    menu.append(&trail_root)?;
    keep_submenus.push(trail_root);

    let sound = CheckMenuItem::with_id(MENU_SOUND, "音效", true, config.sound, None);
    let glow = CheckMenuItem::with_id(MENU_GLOW, "辉光", true, config.glow, None);
    let wander = CheckMenuItem::with_id(MENU_WANDER, "自由走动", true, config.wander, None);
    let idle_animation = CheckMenuItem::with_id(
        MENU_IDLE_ANIMATION,
        "静止时播放走路",
        true,
        config.idle_animation,
        None,
    );
    let follow = CheckMenuItem::with_id(MENU_FOLLOW, "跟随鼠标（20 秒）", true, false, None);
    let summon_root = Submenu::new("召唤", true);
    let summon = MenuItem::with_id(MENU_SUMMON, "召唤到屏幕中心", true, None);
    let summon_animation = CheckMenuItem::with_id(
        MENU_SUMMON_ANIMATION,
        "播放召唤动画",
        true,
        config.summon_animation,
        None,
    );
    summon_root.append(&summon)?;
    summon_root.append(&summon_animation)?;
    summon_root.append(&PredefinedMenuItem::separator())?;
    let start_at_center = CheckMenuItem::with_id(
        MENU_START_AT_CENTER,
        "启动时回到屏幕中心（关闭=记住上次）",
        true,
        config.start_at_center,
        None,
    );
    summon_root.append(&start_at_center)?;
    keep_items.push(summon.clone());
    menu.append(&sound)?;
    menu.append(&glow)?;
    menu.append(&wander)?;
    menu.append(&idle_animation)?;
    menu.append(&follow)?;
    menu.append(&summon_root)?;
    keep_submenus.push(summon_root);
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
        glow,
        wander,
        idle_animation,
        follow,
        summon,
        summon_animation,
        start_at_center,
        display_items,
        form_items,
        scale_items,
        tint_items,
        tint_custom,
        tint_hex,
        speed_move_value,
        speed_follow_value,
        speed_idle_value,
        speed_follow_global,
        speed_idle_global,
        trail_enabled,
        trail_interval_value,
        trail_lifetime_value,
        trail_opacity_value,
        trail_watch,
        keep_items,
        keep_submenus,
    })
}
