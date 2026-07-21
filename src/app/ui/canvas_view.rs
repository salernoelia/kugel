use crate::app::App;
use crate::image_utils::process_file_to_images;
use crate::shapes::{ShapeData, Tool};
use eframe::egui;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;

pub fn render_central_canvas(app: &mut App, ctx: &egui::Context, is_dark: bool) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            let (response, mut painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());

            let mut alignment_guides: Vec<(egui::Pos2, egui::Pos2)> = Vec::new();

            // Render background
            painter.rect_filled(response.rect, 0.0, app.background_color);

            // Draw grid dots
            if app.use_grid {
                let mut grid_spacing = 50.0 * app.zoom;
                while grid_spacing < 24.0 {
                    grid_spacing *= 2.0;
                }
                let grid_color = if is_dark {
                    egui::Color32::from_gray(95).gamma_multiply(0.45)
                } else {
                    egui::Color32::from_gray(130).gamma_multiply(0.6)
                };

                if grid_spacing > 8.0 {
                    let min_x = ((response.rect.min.x - app.pan_offset.x) / grid_spacing)
                        .floor()
                        * grid_spacing
                        + app.pan_offset.x;
                    let min_y = ((response.rect.min.y - app.pan_offset.y) / grid_spacing)
                        .floor()
                        * grid_spacing
                        + app.pan_offset.y;

                    let mut y = min_y;
                    while y < response.rect.max.y {
                        let mut x = min_x;
                        while x < response.rect.max.x {
                            painter.circle_filled(egui::pos2(x, y), 1.0, grid_color);
                            x += grid_spacing;
                        }
                        y += grid_spacing;
                    }
                }
            }

            // Panning and zooming
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            let zoom_delta = ui.input(|i| i.zoom_delta());
            let (mut has_wheel, mut has_trackpad) = (false, false);
            ui.input(|i| {
                for e in &i.events {
                    if let egui::Event::MouseWheel {
                        unit, modifiers, ..
                    } = e
                    {
                        if modifiers.command || modifiers.ctrl {
                            continue;
                        }
                        match unit {
                            egui::MouseWheelUnit::Point => has_trackpad = true,
                            _ => has_wheel = true,
                        }
                    }
                }
            });

            if has_trackpad && !has_wheel {
                app.pan_offset += scroll_delta;
            }

            let wheel_zoom = if has_wheel { scroll_delta.y } else { 0.0 };
            if zoom_delta != 1.0 || wheel_zoom != 0.0 {
                let pointer_pos = response.hover_pos().unwrap_or(response.rect.center());
                let zoom_factor = if zoom_delta != 1.0 {
                    zoom_delta
                } else {
                    1.0 + wheel_zoom * 0.003
                };

                if zoom_factor != 1.0 {
                    let old_zoom = app.zoom;
                    app.zoom = (app.zoom * zoom_factor).clamp(0.5, 10.0);

                    let zoom_change = app.zoom / old_zoom;
                    app.pan_offset = pointer_pos.to_vec2()
                        + (app.pan_offset - pointer_pos.to_vec2()) * zoom_change;
                }
            }

            // Keyboard Shortcuts
            let has_shortcut = |ui: &egui::Ui, key: egui::Key, cmd: bool| {
                ui.input(|i| {
                    i.events.iter().any(|e| match e {
                        egui::Event::Key {
                            key: k,
                            pressed: true,
                            modifiers,
                            ..
                        } if *k == key => !cmd || modifiers.command || modifiers.ctrl,
                        egui::Event::Paste(_) if key == egui::Key::V && cmd => true,
                        egui::Event::Copy if key == egui::Key::C && cmd => true,
                        _ => false,
                    })
                })
            };

            if app.editing_text_index.is_none() {
                let bare_key = |ui: &egui::Ui, key: egui::Key| -> bool {
                    ui.input(|i| {
                        i.key_pressed(key)
                            && !i.modifiers.command
                            && !i.modifiers.ctrl
                            && !i.modifiers.alt
                    })
                };

                if bare_key(ui, egui::Key::V) || bare_key(ui, egui::Key::W) {
                    app.tool = Tool::Select;
                    app.clear_selection();
                }
                if bare_key(ui, egui::Key::P) {
                    app.tool = Tool::Pen;
                    app.clear_selection();
                }
                if bare_key(ui, egui::Key::L) {
                    app.tool = Tool::Line;
                    app.clear_selection();
                }
                if bare_key(ui, egui::Key::R) {
                    app.tool = Tool::Rectangle;
                    app.clear_selection();
                }
                if bare_key(ui, egui::Key::O) {
                    app.tool = Tool::Circle;
                    app.clear_selection();
                }
                if bare_key(ui, egui::Key::T) {
                    app.tool = Tool::Text;
                    app.clear_selection();
                }
                if bare_key(ui, egui::Key::N) {
                    app.tool = Tool::StickyNote;
                    app.clear_selection();
                }
                if bare_key(ui, egui::Key::F) {
                    app.tool = Tool::Section;
                    app.clear_selection();
                }

                if bare_key(ui, egui::Key::I) {
                    app.import_image_dialog(ctx);
                }
            }

            if has_shortcut(ui, egui::Key::Z, true) {
                app.canvas.undo();
                app.clear_selection();
                app.editing_text_index = None;
                app.is_dirty = true;
            }
            if has_shortcut(ui, egui::Key::Y, true) {
                app.canvas.redo();
                app.clear_selection();
                app.editing_text_index = None;
                app.is_dirty = true;
            }
            if has_shortcut(ui, egui::Key::S, true) {
                app.save();
            }
            if has_shortcut(ui, egui::Key::O, true) {
                app.open_file_dialog(ctx);
            }
            if has_shortcut(ui, egui::Key::N, true) {
                app.new_board();
            }
            if has_shortcut(ui, egui::Key::E, true) {
                app.show_export_dialog = true;
            }

            // Select all (Cmd/Ctrl + A)
            if app.editing_text_index.is_none() && has_shortcut(ui, egui::Key::A, true) {
                app.tool = Tool::Select;
                app.select_all();
                app.notification = Some((
                    format!("Selected {} shape(s)", app.canvas.shapes.len()),
                    Instant::now(),
                ));
            }

            // Declutter selection into row (Shift + A)
            if app.editing_text_index.is_none()
                && has_shortcut(ui, egui::Key::A, false)
                && ui.input(|i| i.modifiers.shift)
            {
                app.declutter_selection();
            }

            // Duplicate selection (Cmd/Ctrl + D)
            if has_shortcut(ui, egui::Key::D, true) {
                if let Some(&idx) = app.primary_selected.as_ref() {
                    if idx < app.canvas.shapes.len() {
                        app.canvas.history.push(app.canvas.shapes.clone());
                        app.canvas.undo_history.clear();
                        app.is_dirty = true;

                        let mut dup = app.canvas.shapes[idx].clone();
                        dup.data.translate(egui::vec2(20.0, 20.0));
                        dup.id = app.canvas.next_id;
                        app.canvas.next_id += 1;
                        dup.data.load_textures(ctx, dup.id);

                        app.canvas.shapes.push(dup);
                        app.select_single(app.canvas.shapes.len() - 1);
                        app.notification = Some((
                            "Duplicated selection".to_string(),
                            Instant::now(),
                        ));
                    }
                }
            }

            // Copy selection (Cmd/Ctrl + C)
            if has_shortcut(ui, egui::Key::C, true) {
                if let Some(&idx) = app.primary_selected.as_ref() {
                    if idx < app.canvas.shapes.len() {
                        app.copied_shape = Some(app.canvas.shapes[idx].clone());
                        app.notification = Some((
                            "Copied shape to buffer".to_string(),
                            Instant::now(),
                        ));
                    }
                }
            }

            // Delete Selection
            if ui.input(|i| {
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)
            }) {
                if app.editing_text_index.is_none() && app.has_selection() {
                    app.canvas.history.push(app.canvas.shapes.clone());
                    app.canvas.undo_history.clear();
                    app.is_dirty = true;
                    let mut indices: Vec<usize> =
                        app.selected_shape_indices.iter().copied().collect();
                    indices.sort_unstable_by(|a, b| b.cmp(a));
                    for idx in indices {
                        if idx < app.canvas.shapes.len() {
                            app.canvas.shapes.remove(idx);
                        }
                    }
                    app.clear_selection();
                    app.notification =
                        Some(("Deleted shape(s)".to_string(), Instant::now()));
                }
            }

            // Nudge controls (Arrow keys)
            if app.editing_text_index.is_none() && app.has_selection() {
                let shift = ui.input(|i| i.modifiers.shift);
                let dist = if shift { 10.0 } else { 1.0 };
                let mut nudge_delta = egui::Vec2::ZERO;

                if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                    nudge_delta.x = -dist;
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    nudge_delta.x = dist;
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    nudge_delta.y = -dist;
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    nudge_delta.y = dist;
                }

                if nudge_delta != egui::Vec2::ZERO {
                    app.canvas.history.push(app.canvas.shapes.clone());
                    app.canvas.undo_history.clear();
                    app.is_dirty = true;
                    for &idx in &app.selected_shape_indices {
                        if idx < app.canvas.shapes.len() {
                            app.canvas.shapes[idx].data.translate(nudge_delta);
                        }
                    }
                }
            }

            // Drag and drop files
            let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
            let has_dropped_files = !dropped_files.is_empty();
            if has_dropped_files {
                let mut image_paths: Vec<std::path::PathBuf> = Vec::new();
                for file in dropped_files {
                    if let Some(path) = &file.path {
                        if path.extension().map_or(false, |ext| ext == "kugel") {
                            let mut proceed = true;
                            if !app.canvas.shapes.is_empty() {
                                let confirm = rfd::MessageDialog::new()
                                    .set_title("Unsaved Changes")
                                    .set_description(
                                        "Do you want to save your current board first?",
                                    )
                                    .set_buttons(rfd::MessageButtons::YesNoCancel)
                                    .show();

                                match confirm {
                                    rfd::MessageDialogResult::Yes => {
                                        proceed = app.save();
                                    }
                                    rfd::MessageDialogResult::No => {
                                        proceed = true;
                                    }
                                    _ => {
                                        proceed = false;
                                    }
                                }
                            }

                            if proceed {
                                app.open_kugel_file(path, ctx);
                            }
                        } else {
                            image_paths.push(path.clone());
                        }
                    }
                }

                if !image_paths.is_empty() {
                    let all_images: Vec<(Vec<u8>, [f32; 2])> = std::thread::scope(|s| {
                        let handles: Vec<_> = image_paths
                            .iter()
                            .map(|path| {
                                s.spawn(move || process_file_to_images(path))
                            })
                            .collect();
                        handles
                            .into_iter()
                            .flat_map(|h| h.join().unwrap_or_default())
                            .collect()
                    });

                    if !all_images.is_empty() {
                        let count = all_images.len();
                        let target_canvas = app.paste_target_canvas(ctx);
                        app.place_images_in_row(all_images, target_canvas, ctx);
                        app.notification = Some((
                            if count == 1 {
                                "Imported image".to_string()
                            } else {
                                format!("Imported {} images/pages in a row", count)
                            },
                            Instant::now(),
                        ));
                    }
                }
            }

            // Panning gesture logic
            let is_panning = ui.input(|i| {
                i.pointer.middle_down()
                    || (i.key_down(egui::Key::Space) && i.pointer.primary_down())
            });

            if is_panning && response.dragged() {
                app.pan_offset += response.drag_delta();
            } else if !is_panning {
                if app.tool == Tool::Select && ui.input(|i| i.pointer.any_released()) {
                    if let Some(start_canvas) = app.marquee_start {
                        let latest_pos = ui.input(|i| i.pointer.latest_pos());
                        let end_canvas = if let Some(p) = latest_pos {
                            app.screen_to_canvas(p)
                        } else {
                            if let Some(pos) =
                                response.hover_pos().or(response.interact_pointer_pos())
                            {
                                app.screen_to_canvas(pos)
                            } else {
                                start_canvas
                            }
                        };

                        let marquee_box = egui::Rect::from_two_pos(start_canvas, end_canvas);
                        if marquee_box.width() > 2.0 && marquee_box.height() > 2.0 {
                            app.clear_selection();
                            for (idx, shape) in app.canvas.shapes.iter().enumerate() {
                                let is_section = matches!(shape.data, ShapeData::SectionBox { .. });
                                let shape_bounds = shape.data.get_bounds();
                                let selected = if is_section {
                                    marquee_box.contains_rect(shape_bounds)
                                } else {
                                    marquee_box.intersects(shape_bounds)
                                };
                                if selected {
                                    app.selected_shape_indices.insert(idx);
                                    app.primary_selected = Some(idx);
                                }
                            }
                        }
                    }
                    app.is_resizing = None;
                    app.is_dragging_shape = false;
                    app.snap_correction = egui::Vec2::ZERO;
                    app.marquee_start = None;
                    app.is_dirty = true;
                }

                let pointer_pos = response.hover_pos().or(response.interact_pointer_pos());
                if let Some(pos) = pointer_pos {
                    let canvas_pos = app.screen_to_canvas(pos);

                    if app.tool == Tool::Select {
                        let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
                        let press_pos = ui.input(|i| i.pointer.press_origin());

                        if !has_dropped_files
                            && primary_pressed
                            && press_pos.is_some()
                            && response.rect.contains(press_pos.unwrap())
                        {
                            let click_pos = press_pos.unwrap();
                            let click_canvas_pos = app.screen_to_canvas(click_pos);

                            if let Some(edit_idx) = app.editing_text_index {
                                let clicked_edited_shape = edit_idx < app.canvas.shapes.len()
                                    && app.canvas.shapes[edit_idx]
                                        .data
                                        .contains_point(click_canvas_pos, 5.0);
                                if !clicked_edited_shape {
                                    app.editing_text_index = None;
                                    app.request_text_focus = false;
                                    app.tool = Tool::Select;
                                }
                            }

                            let mut clicked_handle = false;

                            if app.selected_shape_indices.len() > 1 {
                                if let Some(handle_idx) =
                                    app.group_handle_under_mouse(click_pos)
                                {
                                    app.canvas.push_history();
                                    app.is_resizing = Some(handle_idx);
                                    app.drag_start_pos = click_pos;
                                    clicked_handle = true;
                                }
                            } else if let Some(selected_idx) = app.primary_selected {
                                if selected_idx < app.canvas.shapes.len() {
                                    if let Some(handle_idx) =
                                        app.get_handle_under_mouse(selected_idx, click_pos)
                                    {
                                        app.canvas.push_history();
                                        app.is_resizing = Some(handle_idx);
                                        app.drag_start_pos = click_pos;
                                        clicked_handle = true;
                                    }
                                }
                            }

                            if !clicked_handle {
                                if let Some(idx) = app.hit_test(click_canvas_pos) {
                                    let shift = ui.input(|i| i.modifiers.shift);
                                    if shift {
                                        if app.selected_shape_indices.contains(&idx) {
                                            app.selected_shape_indices.remove(&idx);
                                            if app.primary_selected == Some(idx) {
                                                app.primary_selected =
                                                    app.selected_shape_indices.iter().next().copied();
                                            }
                                        } else {
                                            app.selected_shape_indices.insert(idx);
                                            app.primary_selected = Some(idx);
                                        }
                                    } else if !app.selected_shape_indices.contains(&idx) {
                                        app.select_single(idx);
                                    }
                                    if ui.input(|i| i.modifiers.alt) {
                                        app.duplicate_selection(ctx);
                                    }
                                    app.canvas.push_history();
                                    app.is_dragging_shape = true;
                                    app.drag_start_pos = click_pos;
                                    app.snap_correction = egui::Vec2::ZERO;
                                    app.marquee_start = None;
                                } else {
                                    if !ui.input(|i| i.modifiers.shift) {
                                        app.clear_selection();
                                    }
                                    app.marquee_start = Some(click_canvas_pos);
                                }
                            }
                        }

                        if response.hovered() {
                            let cmd = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
                            if cmd {
                                if let Some(idx) = app.hit_test(canvas_pos) {
                                    if app.text_shape_url(idx).is_some() {
                                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                }
                            }
                        }

                        if response.clicked() && !has_dropped_files {
                            if let Some(idx) = app.hit_test(canvas_pos) {
                                let cmd = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
                                let shift = ui.input(|i| i.modifiers.shift);
                                let url = if cmd { app.text_shape_url(idx) } else { None };
                                if let Some(url) = url {
                                    ctx.open_url(egui::OpenUrl::new_tab(url));
                                } else if !shift {
                                    app.select_single(idx);
                                }
                                app.marquee_start = None;
                            } else if !ui.input(|i| i.modifiers.shift) {
                                app.clear_selection();
                            }
                        }

                        if ui.input(|i| {
                            i.pointer
                                .button_double_clicked(egui::PointerButton::Primary)
                        }) && response.hovered()
                        {
                            if let Some(idx) = app.hit_test(canvas_pos) {
                                let text_opt = match &app.canvas.shapes[idx].data {
                                    ShapeData::Text { text, .. }
                                    | ShapeData::StickyNote { text, .. } => Some(text.clone()),
                                    _ => None,
                                };
                                if let Some(text) = text_opt {
                                    app.canvas.push_history();
                                    app.editing_text_index = Some(idx);
                                    app.editing_text_buffer = text;
                                    app.request_text_focus = true;
                                    app.select_single(idx);
                                    app.tool = Tool::Select;
                                    app.marquee_start = None;
                                }
                            }
                        }

                        if response.dragged() {
                            let delta = response.drag_delta() / app.zoom;
                            if let Some(handle_idx) = app.is_resizing {
                                if app.selected_shape_indices.len() > 1 {
                                    if let Some(bounds) = app.selection_bounds() {
                                        let anchor = match handle_idx {
                                            0 => bounds.right_bottom(),
                                            1 => bounds.left_bottom(),
                                            2 => bounds.right_top(),
                                            _ => bounds.left_top(),
                                        };
                                        let corner = match handle_idx {
                                            0 => bounds.left_top(),
                                            1 => bounds.right_top(),
                                            2 => bounds.left_bottom(),
                                            _ => bounds.right_bottom(),
                                        };
                                        let old_dist = corner.distance(anchor);
                                        let new_dist = canvas_pos.distance(anchor);
                                        if old_dist > 1.0 {
                                            let factor = (new_dist / old_dist).clamp(0.2, 5.0);
                                            if (factor - 1.0).abs() > 0.0001 {
                                                for &idx in &app.selected_shape_indices {
                                                    if idx < app.canvas.shapes.len() {
                                                        app.canvas.shapes[idx]
                                                            .data
                                                            .scale_about(anchor, factor);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else if let Some(primary_idx) = app.primary_selected {
                                    if primary_idx < app.canvas.shapes.len() {
                                        app.canvas.shapes[primary_idx]
                                            .data
                                            .resize(handle_idx, delta, canvas_pos);
                                    }
                                }
                            } else if app.is_dragging_shape {
                                let to_raw = delta - app.snap_correction;
                                for &idx in &app.selected_shape_indices {
                                    if idx < app.canvas.shapes.len() {
                                        app.canvas.shapes[idx].data.translate(to_raw);
                                    }
                                }
                                let mut correction = egui::Vec2::ZERO;
                                if let Some(p) = app.primary_selected {
                                    if p < app.canvas.shapes.len() {
                                        let moving = app.canvas.shapes[p].data.get_bounds();
                                        let (corr, guides) = app
                                            .compute_alignment_snap(moving, 6.0 / app.zoom);
                                        correction = corr;
                                        alignment_guides = guides;
                                    }
                                }
                                if correction != egui::Vec2::ZERO {
                                    for &idx in &app.selected_shape_indices {
                                        if idx < app.canvas.shapes.len() {
                                            app.canvas.shapes[idx].data.translate(correction);
                                        }
                                    }
                                }
                                app.snap_correction = correction;
                            }
                        }

                        if let Some(start_canvas) = app.marquee_start {
                            let start_screen = app.canvas_to_screen(start_canvas);
                            let rect_screen = egui::Rect::from_two_pos(start_screen, pos);
                            painter.rect_filled(
                                rect_screen,
                                2.0,
                                egui::Color32::from_rgb(99, 102, 241).gamma_multiply(0.12),
                            );
                            painter.rect_stroke(
                                rect_screen,
                                2.0,
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(99, 102, 241)),
                                egui::StrokeKind::Outside,
                            );
                        }

                        if app.selected_shape_indices.len() > 1 {
                            if let Some(handle_idx) = app.group_handle_under_mouse(pos) {
                                let cursor = match handle_idx {
                                    0 | 3 => egui::CursorIcon::ResizeNwSe,
                                    _ => egui::CursorIcon::ResizeNeSw,
                                };
                                ctx.set_cursor_icon(cursor);
                            }
                        } else if let Some(selected_idx) = app.primary_selected {
                            if selected_idx < app.canvas.shapes.len() {
                                if let Some(handle_idx) =
                                    app.get_handle_under_mouse(selected_idx, pos)
                                {
                                    let is_text_or_sticky = matches!(
                                        app.canvas.shapes[selected_idx].data,
                                        ShapeData::Text { .. } | ShapeData::StickyNote { .. }
                                    );
                                    let cursor = if is_text_or_sticky {
                                        egui::CursorIcon::ResizeHorizontal
                                    } else {
                                        match handle_idx {
                                            0 | 3 => egui::CursorIcon::ResizeNwSe,
                                            1 | 2 => egui::CursorIcon::ResizeNeSw,
                                            _ => egui::CursorIcon::Default,
                                        }
                                    };
                                    ctx.set_cursor_icon(cursor);
                                }
                            }
                        }
                    } else {
                        if (app.tool == Tool::Text || app.tool == Tool::StickyNote)
                            && response.clicked()
                        {
                            let edit_existing = app.hit_test(canvas_pos).filter(|&idx| {
                                matches!(
                                    app.canvas.shapes[idx].data,
                                    ShapeData::Text { .. } | ShapeData::StickyNote { .. }
                                )
                            });
                            if let Some(idx) = edit_existing {
                                let text = match &app.canvas.shapes[idx].data {
                                    ShapeData::Text { text, .. }
                                    | ShapeData::StickyNote { text, .. } => text.clone(),
                                    _ => String::new(),
                                };
                                app.canvas.push_history();
                                app.editing_text_index = Some(idx);
                                app.editing_text_buffer = text;
                                app.request_text_focus = true;
                                app.select_single(idx);
                                app.tool = Tool::Select;
                                app.marquee_start = None;
                            } else {
                                let edit_idx = app.canvas.start_shape(
                                    app.tool,
                                    canvas_pos,
                                    app.selected_color,
                                    app.stroke_width,
                                    app.filled_shapes,
                                );
                                if let Some(idx) = edit_idx {
                                    app.editing_text_index = Some(idx);
                                    app.editing_text_buffer = String::new();
                                    app.request_text_focus = true;
                                    app.select_single(idx);
                                    app.tool = Tool::Select;
                                }
                            }
                        } else if app.tool != Tool::Text
                            && app.tool != Tool::StickyNote
                            && response.drag_started()
                        {
                            let edit_idx = app.canvas.start_shape(
                                app.tool,
                                canvas_pos,
                                app.selected_color,
                                app.stroke_width,
                                app.filled_shapes,
                            );
                            if let Some(idx) = edit_idx {
                                app.editing_text_index = Some(idx);
                                app.editing_text_buffer = String::new();
                                app.request_text_focus = true;
                                app.select_single(idx);
                            }
                        }

                        if response.dragged() {
                            app.canvas.update_current_shape(canvas_pos);
                        }

                        if response.drag_stopped() {
                            if let Some(idx) = app.canvas.finish_shape() {
                                app.select_single(idx);
                                app.tool = Tool::Select;
                            }
                            app.is_dirty = true;
                        }
                    }
                }
            }

            // Draw canvas elements
            painter.set_clip_rect(response.rect);
            app.canvas.render(
                &painter,
                app.zoom,
                app.pan_offset,
                app.editing_text_index,
            );

            // Draw selection box & resize handles
            if app.tool == Tool::Select {
                for &idx in &app.selected_shape_indices {
                    if idx < app.canvas.shapes.len() {
                        let bounds = app.canvas.shapes[idx].data.get_bounds();
                        if bounds.is_positive() {
                            let screen_bounds = egui::Rect::from_min_max(
                                app.canvas_to_screen(bounds.min),
                                app.canvas_to_screen(bounds.max),
                            );

                            painter.rect_stroke(
                                screen_bounds,
                                0.0,
                                egui::Stroke::new(1.5, egui::Color32::from_rgb(99, 102, 241)),
                                egui::StrokeKind::Outside,
                            );

                            if app.primary_selected == Some(idx)
                                && app.selected_shape_indices.len() == 1
                            {
                                let is_text_or_sticky = matches!(
                                    app.canvas.shapes[idx].data,
                                    ShapeData::Text { .. } | ShapeData::StickyNote { .. }
                                );
                                let handle_positions = if is_text_or_sticky {
                                    vec![
                                        screen_bounds.right_top(),
                                        screen_bounds.right_bottom(),
                                    ]
                                } else {
                                    vec![
                                        screen_bounds.left_top(),
                                        screen_bounds.right_top(),
                                        screen_bounds.left_bottom(),
                                        screen_bounds.right_bottom(),
                                    ]
                                };
                                for &h_pos in &handle_positions {
                                    painter.rect(
                                        egui::Rect::from_center_size(
                                            h_pos,
                                            egui::vec2(8.0, 8.0),
                                        ),
                                        2.0,
                                        egui::Color32::WHITE,
                                        egui::Stroke::new(
                                            1.5,
                                            egui::Color32::from_rgb(99, 102, 241),
                                        ),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                            }
                        }
                    }
                }

                if app.selected_shape_indices.len() > 1 {
                    if let Some(bounds) = app.selection_bounds() {
                        let screen_bounds = egui::Rect::from_min_max(
                            app.canvas_to_screen(bounds.min),
                            app.canvas_to_screen(bounds.max),
                        );
                        painter.rect_stroke(
                            screen_bounds,
                            0.0,
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(99, 102, 241)),
                            egui::StrokeKind::Outside,
                        );
                        let corners = [
                            screen_bounds.left_top(),
                            screen_bounds.right_top(),
                            screen_bounds.left_bottom(),
                            screen_bounds.right_bottom(),
                        ];
                        for &c in &corners {
                            painter.rect(
                                egui::Rect::from_center_size(c, egui::vec2(8.0, 8.0)),
                                2.0,
                                egui::Color32::WHITE,
                                egui::Stroke::new(1.5, egui::Color32::from_rgb(99, 102, 241)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                }
            }

            // Alignment guides
            for (a, b) in &alignment_guides {
                painter.line_segment(
                    [app.canvas_to_screen(*a), app.canvas_to_screen(*b)],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 60, 120)),
                );
            }

            // Text dimensions caching & StickyNote auto-resizing
            for shape in &mut app.canvas.shapes {
                match &mut shape.data {
                    ShapeData::Text {
                        text,
                        size,
                        max_width,
                        cached_size,
                        cache_key,
                        ..
                    } => {
                        let mut hasher = DefaultHasher::new();
                        text.hash(&mut hasher);
                        size.to_bits().hash(&mut hasher);
                        max_width.map(|w| w.to_bits()).hash(&mut hasher);
                        let key = hasher.finish();
                        if *cache_key != Some(key) || cached_size.is_none() {
                            let font_id = egui::FontId::proportional(*size);
                            let galley = if let Some(mw) = max_width {
                                ui.fonts(|f| {
                                    f.layout(text.clone(), font_id, egui::Color32::WHITE, *mw)
                                })
                            } else {
                                ui.fonts(|f| {
                                    f.layout_no_wrap(
                                        text.clone(),
                                        font_id,
                                        egui::Color32::WHITE,
                                    )
                                })
                            };
                            *cached_size = Some(galley.size());
                            *cache_key = Some(key);
                        }
                    }
                    ShapeData::StickyNote {
                        rect,
                        text,
                        text_size,
                        cached_height,
                        cache_key,
                        ..
                    } => {
                        let padding = 16.0;
                        let text_width = (rect.width() - padding).max(10.0);
                        let mut hasher = DefaultHasher::new();
                        text.hash(&mut hasher);
                        text_size.to_bits().hash(&mut hasher);
                        text_width.to_bits().hash(&mut hasher);
                        let key = hasher.finish();
                        let required_height = if *cache_key == Some(key) {
                            cached_height.unwrap_or(140.0)
                        } else {
                            let font_id = egui::FontId::proportional(*text_size);
                            let galley = ui.fonts(|f| {
                                f.layout(
                                    text.clone(),
                                    font_id,
                                    egui::Color32::WHITE,
                                    text_width,
                                )
                            });
                            let h = (galley.size().y + padding).max(140.0);
                            *cached_height = Some(h);
                            *cache_key = Some(key);
                            h
                        };
                        if (rect.height() - required_height).abs() > 0.1 {
                            rect.max.y = rect.min.y + required_height;
                            app.is_dirty = true;
                        }
                    }
                    _ => {}
                }
            }
        });
}
