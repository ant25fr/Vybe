//! The banner of the graphic charter (`docs/charte-graphique/img.png`),
//! drawn as shapes for the About dialog: the name over the application's
//! function, a pulse of rings and the track colours.

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas;
use iced::{border, mouse, Color, Point, Rectangle, Renderer, Size, Theme};

use crate::message::Message;
use crate::typography;

/// The banner's own size, which every coordinate below is given in.
pub const SIZE: Size = Size::new(959.0, 290.0);
const TOP: f32 = 2.0;
const BOTTOM: f32 = 288.0;
const PANEL_X: f32 = 481.0;
const STRIP_X: f32 = 913.0;
const BAR_Y: f32 = 258.0;
const CORNER: f32 = 22.0;
const CENTRE: Point = Point::new(721.0, 130.0);
/// Radii of the quiet rings; the loud one sits at [`ACCENT_RING`].
const RINGS: [f32; 6] = [36.0, 59.0, 82.0, 105.0, 151.0, 174.0];
const ACCENT_RING: f32 = 128.0;

pub const GROUND: Color = rgb(16, 18, 20);
const PANEL: Color = rgb(40, 42, 45);
const RING: Color = rgb(86, 88, 91);
pub const ACCENT: Color = rgb(253, 135, 30);
const SUBTITLE: Color = rgb(175, 177, 180);
const BAR: [Color; 8] = [
    rgb(237, 117, 110),
    rgb(236, 146, 71),
    rgb(223, 195, 81),
    rgb(94, 192, 119),
    rgb(57, 188, 195),
    rgb(97, 158, 234),
    rgb(163, 127, 222),
    rgb(226, 116, 171),
];

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// The runs of the circle of `radius` around [`CENTRE`] that lie in the
/// panel, as points in the banner's own coordinates.
fn arcs_within_panel(radius: f32) -> Vec<Vec<Point>> {
    const STEPS: u16 = 720;
    let mut arcs: Vec<Vec<Point>> = Vec::new();
    let mut open = false;
    for step in 0..=STEPS {
        let angle = f32::from(step) / f32::from(STEPS) * std::f32::consts::TAU;
        let point = Point::new(
            CENTRE.x + radius * angle.cos(),
            CENTRE.y + radius * angle.sin(),
        );
        let inside = (PANEL_X..=STRIP_X).contains(&point.x) && (TOP..=BAR_Y).contains(&point.y);
        match arcs.last_mut() {
            Some(arc) if inside && open => arc.push(point),
            _ if inside => arcs.push(vec![point]),
            _ => {}
        }
        open = inside;
    }
    arcs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_are_cut_where_they_leave_the_panel() {
        // The smallest ring is whole, the largest loses its top and bottom.
        assert_eq!(arcs_within_panel(RINGS[0]).len(), 1);
        let outer = arcs_within_panel(RINGS[5]);
        assert!(outer.len() > 1);
        assert!(outer.iter().flatten().all(|p| p.y >= TOP && p.y <= BAR_Y));
    }
}

pub struct AboutBanner;

impl canvas::Program<Message> for AboutBanner {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let s = bounds.width / SIZE.width;
        let at = |x: f32, y: f32| Point::new(x * s, y * s);
        let area = |x: f32, y: f32, width: f32, height: f32| {
            Rectangle::new(at(x, y), Size::new(width * s, height * s))
        };

        frame.fill(
            &canvas::Path::rounded_rectangle(
                at(0.0, TOP),
                Size::new(SIZE.width * s, (BOTTOM - TOP) * s),
                (CORNER * s).into(),
            ),
            GROUND,
        );

        let panel = area(PANEL_X, TOP, STRIP_X - PANEL_X, BAR_Y - TOP);
        frame.fill_rectangle(panel.position(), panel.size(), PANEL);
        // The rings are cut to the panel here: a clipped frame would be
        // painted under the fills above on the GPU renderer.
        let loud = (ACCENT_RING, ACCENT, 6.0);
        for (radius, color, width) in RINGS.map(|r| (r, RING, 2.0)).into_iter().chain([loud]) {
            let path = canvas::Path::new(|path| {
                for arc in arcs_within_panel(radius) {
                    path.move_to(at(arc[0].x, arc[0].y));
                    for point in &arc[1..] {
                        path.line_to(at(point.x, point.y));
                    }
                }
            });
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(color)
                    .with_width(width * s),
            );
        }
        frame.fill(
            &canvas::Path::circle(at(CENTRE.x, CENTRE.y), 12.0 * s),
            ACCENT,
        );

        let strip = area(STRIP_X, TOP, SIZE.width - STRIP_X, BAR_Y - TOP);
        frame.fill(
            &canvas::Path::rounded_rectangle(
                strip.position(),
                strip.size(),
                border::Radius {
                    top_right: CORNER * s,
                    ..Default::default()
                },
            ),
            ACCENT,
        );

        let segment = (SIZE.width - PANEL_X) / BAR.len() as f32;
        for (index, color) in BAR.into_iter().enumerate() {
            let place = area(
                PANEL_X + segment * index as f32,
                BAR_Y,
                segment,
                BOTTOM - BAR_Y,
            );
            let corner = if index + 1 == BAR.len() {
                CORNER * s
            } else {
                0.0
            };
            frame.fill(
                &canvas::Path::rounded_rectangle(
                    place.position(),
                    place.size(),
                    border::Radius {
                        bottom_right: corner,
                        ..Default::default()
                    },
                ),
                color,
            );
        }

        let mut write = |content: &str, y: f32, size: f32, font, color| {
            frame.fill_text(canvas::Text {
                content: content.to_string(),
                position: at(41.0, y),
                color,
                size: (size * s).into(),
                font,
                horizontal_alignment: Horizontal::Left,
                vertical_alignment: Vertical::Center,
                ..canvas::Text::default()
            });
        };
        write("vibez", 181.0, 110.0, typography::PERFORM_DISPLAY, ACCENT);
        write(
            "Digital Audio Workstation",
            250.0,
            15.0,
            typography::PERFORM_LABEL,
            SUBTITLE,
        );
        vec![frame.into_geometry()]
    }
}
