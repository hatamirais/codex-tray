use std::f32::consts::PI;

use tiny_skia::{Color, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

use crate::{tray, usage::CodexUsage};

const TRACK: [u8; 3] = [0x34, 0x34, 0x38];
const NEUTRAL: [u8; 3] = [0x8e, 0x8e, 0x93];

pub fn render(usage: &CodexUsage, size: u32) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(size, size)?;
    let session = usage.session_remaining_percent.map(tray::rounded_percent);
    let weekly = usage.weekly_remaining_percent.map(tray::rounded_percent);

    draw_arc(&mut pixmap, -135.0, 180.0, TRACK);
    draw_arc(&mut pixmap, 45.0, 180.0, TRACK);

    match (session, weekly) {
        (None, None) => {
            draw_arc(&mut pixmap, -135.0, 180.0, NEUTRAL);
            draw_arc(&mut pixmap, 45.0, 180.0, NEUTRAL);
        }
        (session, weekly) => {
            if let Some(percent) = session {
                draw_arc(
                    &mut pixmap,
                    -135.0,
                    180.0 * f32::from(percent) / 100.0,
                    tray::usage_color(percent),
                );
            }
            if let Some(percent) = weekly {
                draw_arc(
                    &mut pixmap,
                    45.0,
                    180.0 * f32::from(percent) / 100.0,
                    tray::usage_color(percent),
                );
            }
        }
    }

    Some(pixmap)
}

fn draw_arc(pixmap: &mut Pixmap, start_degrees: f32, sweep_degrees: f32, rgb: [u8; 3]) {
    if sweep_degrees <= 0.0 {
        return;
    }

    let size = pixmap.width() as f32;
    let center = size / 2.0;
    let radius = size * 23.0 / 64.0;
    let path = arc_path(center, radius, start_degrees, sweep_degrees);
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255));
    paint.anti_alias = true;
    let stroke = Stroke {
        width: size * 8.0 / 64.0,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

fn arc_path(center: f32, radius: f32, start_degrees: f32, sweep_degrees: f32) -> Path {
    let steps = ((sweep_degrees / 6.0).ceil() as usize).max(1);
    let start = start_degrees * PI / 180.0;
    let sweep = sweep_degrees * PI / 180.0;
    let mut path = PathBuilder::new();

    for step in 0..=steps {
        let angle = start + sweep * step as f32 / steps as f32;
        let x = center + radius * angle.cos();
        let y = center + radius * angle.sin();
        if step == 0 {
            path.move_to(x, y);
        } else {
            path.line_to(x, y);
        }
    }
    path.finish()
        .expect("an arc always contains at least two points")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_dynamic_and_fallback_icons() {
        let usage = CodexUsage {
            session_remaining_percent: Some(78.0),
            weekly_remaining_percent: Some(18.0),
            session_reset_at: None,
            weekly_reset_at: None,
        };

        let dynamic = render(&usage, 16).unwrap();
        assert_eq!((dynamic.width(), dynamic.height()), (16, 16));
        let fallback = render(&CodexUsage::unavailable(), 32).unwrap();
        assert_eq!((fallback.width(), fallback.height()), (32, 32));
    }
}
