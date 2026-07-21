use eframe::egui;

#[derive(Clone)]
pub struct IconPair {
    pub light: egui::TextureHandle,
    pub dark: egui::TextureHandle,
}

#[derive(Clone)]
pub struct Icons {
    pub select: IconPair,
    pub pen: IconPair,
    pub line: IconPair,
    pub rectangle: IconPair,
    pub circle: IconPair,
    pub text: IconPair,
    pub note: IconPair,
    pub section: IconPair,
    pub import: IconPair,
    pub undo: IconPair,
    pub redo: IconPair,
    pub clear: IconPair,
    pub save: IconPair,
    pub open: IconPair,
    pub export: IconPair,
    pub theme_dark: IconPair,
    pub theme_light: IconPair,
    pub settings: IconPair,
    pub paintbrush: IconPair,
    pub wallpaper: IconPair,
}

impl Icons {
    pub fn new(ctx: &egui::Context) -> Self {
        let load = |name: &str, bytes: &[u8]| -> IconPair {
            let img = image::load_from_memory(bytes).expect("Failed to load icon from memory");
            let rgba = img.to_rgba8();
            let color_img_light = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                &rgba.into_raw(),
            );

            let mut color_img_dark = color_img_light.clone();
            for pixel in &mut color_img_dark.pixels {
                *pixel = egui::Color32::from_rgba_unmultiplied(
                    255 - pixel.r(),
                    255 - pixel.g(),
                    255 - pixel.b(),
                    pixel.a(),
                );
            }

            let light = ctx.load_texture(
                format!("icon_light_{}", name),
                color_img_light,
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    mipmap_mode: Some(egui::TextureFilter::Linear),
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                },
            );

            let dark = ctx.load_texture(
                format!("icon_dark_{}", name),
                color_img_dark,
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    mipmap_mode: Some(egui::TextureFilter::Linear),
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                },
            );

            IconPair { light, dark }
        };

        Self {
            select: load(
                "select",
                include_bytes!("../assets/icons/mouse-pointer-2.png"),
            ),
            pen: load("pen", include_bytes!("../assets/icons/pen.png")),
            line: load("line", include_bytes!("../assets/icons/line.png")),
            rectangle: load(
                "rectangle",
                include_bytes!("../assets/icons/rectangle-horizontal.png"),
            ),
            circle: load("circle", include_bytes!("../assets/icons/circle.png")),
            text: load("text", include_bytes!("../assets/icons/text-initial.png")),
            note: load("note", include_bytes!("../assets/icons/sticky-note.png")),
            section: load(
                "section",
                include_bytes!("../assets/icons/square-dashed.png"),
            ),
            import: load("import", include_bytes!("../assets/icons/import.png")),
            undo: load("undo", include_bytes!("../assets/icons/undo.png")),
            redo: load("redo", include_bytes!("../assets/icons/redo.png")),
            clear: load("clear", include_bytes!("../assets/icons/trash.png")),
            save: load("save", include_bytes!("../assets/icons/save.png")),
            open: load("open", include_bytes!("../assets/icons/folder-open.png")),
            export: load("export", include_bytes!("../assets/icons/file-down.png")),
            theme_dark: load("theme_dark", include_bytes!("../assets/icons/moon.png")),
            theme_light: load("theme_light", include_bytes!("../assets/icons/sun.png")),
            settings: load("settings", include_bytes!("../assets/icons/settings.png")),
            paintbrush: load(
                "paintbrush",
                include_bytes!("../assets/icons/paintbrush.png"),
            ),
            wallpaper: load("wallpaper", include_bytes!("../assets/icons/wallpaper.png")),
        }
    }

    pub fn icon_button(&self, ui: &mut egui::Ui, pair: &IconPair, tooltip: &str) -> egui::Response {
        let is_dark = ui.visuals().dark_mode;
        let texture = if is_dark { &pair.dark } else { &pair.light };
        let image = egui::Image::new(texture).fit_to_exact_size(egui::vec2(22.0, 22.0));
        ui.add(egui::ImageButton::new(image).frame(false))
            .on_hover_text(tooltip)
    }

    pub fn selectable_icon_button(
        &self,
        ui: &mut egui::Ui,
        selected: bool,
        pair: &IconPair,
        tooltip: &str,
    ) -> egui::Response {
        let is_dark = ui.visuals().dark_mode;
        let texture = if is_dark { &pair.dark } else { &pair.light };
        let image = egui::Image::new(texture).fit_to_exact_size(egui::vec2(22.0, 22.0));
        ui.add(
            egui::ImageButton::new(image)
                .selected(selected)
                .frame(false),
        )
        .on_hover_text(tooltip)
    }
}
