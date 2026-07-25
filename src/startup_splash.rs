use crate::performance_meter::{self, BarCell, LedState, MeterColor};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Clear, Paragraph},
    Frame,
};
use std::time::Duration;

pub const MINIMUM_VISIBLE: Duration = Duration::from_secs(3);
pub const INPUT_RESCAN_INTERVAL: Duration = Duration::from_millis(500);
const INDICATOR_SWEEP: Duration = Duration::from_millis(2_500);
const TITLE_STEP: Duration = Duration::from_millis(75);
const INDICATOR_COUNT: usize = 6;
const INDICATOR_WIDTH: usize = 5;
const INDICATOR_GAP: usize = 2;

pub const fn qualified_input_available(terminal_keyboard: bool, midi_input: bool) -> bool {
    terminal_keyboard || midi_input
}

pub fn waiting_for_input(elapsed: Duration, terminal_keyboard: bool, midi_input: bool) -> bool {
    elapsed < MINIMUM_VISIBLE || !qualified_input_available(terminal_keyboard, midi_input)
}

fn meter_color(color: MeterColor) -> Color {
    match color {
        MeterColor::Green => Color::Green,
        MeterColor::Yellow => Color::LightYellow,
        MeterColor::Red => Color::Red,
    }
}

fn styled_bar(cells: &[BarCell]) -> Vec<Span<'static>> {
    cells
        .iter()
        .map(|cell| {
            let active = cell.state != LedState::Off;
            Span::styled(
                "●",
                Style::default()
                    .fg(if active {
                        meter_color(cell.color)
                    } else {
                        Color::DarkGray
                    })
                    .add_modifier(if active {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            )
        })
        .collect()
}

fn animated_level(elapsed: Duration) -> f32 {
    // One deterministic envelope keeps the decorative channels coherent. The
    // splash previews the LED language; it does not pretend to meter audio.
    const LEVELS: [f32; 16] = [
        -36.0, -31.0, -22.0, -15.0, -8.5, -5.0, -11.0, -18.0, -13.0, -7.0, -2.4, -9.0, -17.0,
        -24.0, -19.0, -12.0,
    ];
    let frame = (elapsed.as_millis() / 85) as usize;
    LEVELS[frame % LEVELS.len()]
}

fn thick_meter(label: char, width: u16, elapsed: Duration) -> Vec<Spans<'static>> {
    let bar_width = usize::from(width.saturating_sub(4)).max(1);
    let rms = animated_level(elapsed);
    let cells = performance_meter::audio_bar(bar_width, rms, performance_meter::AUDIO_FLOOR_DBFS);
    (0..2)
        .map(|row| {
            let mut spans = vec![Span::styled(
                if row == 0 {
                    format!("{label} [")
                } else {
                    "  [".into()
                },
                Style::default().fg(Color::White).add_modifier(if row == 0 {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            )];
            spans.extend(styled_bar(&cells));
            spans.push(Span::styled("]", Style::default().fg(Color::White)));
            Spans::from(spans)
        })
        .collect()
}

fn rows_from_top(area: Rect, first_row: u16, height: u16) -> Option<Rect> {
    if first_row >= area.height || height == 0 {
        return None;
    }
    Some(Rect::new(
        area.x,
        area.y + first_row,
        area.width,
        height.min(area.height - first_row),
    ))
}

fn title(elapsed: Duration) -> Spans<'static> {
    const TITLE: &str = "shr - daw";
    const HIGHLIGHTS: [usize; 7] = [0, 1, 2, 4, 6, 7, 8];
    let step = (elapsed.as_millis() / TITLE_STEP.as_millis()) as usize;
    let highlighted = HIGHLIGHTS[step % HIGHLIGHTS.len()];
    Spans::from(
        TITLE
            .chars()
            .enumerate()
            .map(|(index, character)| {
                Span::styled(
                    character.to_string(),
                    Style::default()
                        .fg(if index == highlighted {
                            Color::LightCyan
                        } else {
                            Color::White
                        })
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn indicator_lit(elapsed: Duration, index: usize, ready: bool) -> bool {
    let threshold_millis =
        INDICATOR_SWEEP.as_millis() * (index + 1) as u128 / INDICATOR_COUNT as u128;
    ready && elapsed.as_millis() >= threshold_millis
}

fn indicator_row(
    width: u16,
    elapsed: Duration,
    input_available: bool,
    controller_checked: bool,
    build_badge: &str,
) -> Spans<'static> {
    let (build_label, build_color) = if build_badge == "DEV" {
        ("DEV", Color::LightBlue)
    } else {
        ("REL", Color::Green)
    };
    let indicators = [
        (build_label, true, build_color),
        ("CFG", true, Color::Green),
        ("SND", true, Color::Green),
        ("TTY", true, Color::Green),
        ("CTL", controller_checked, Color::Green),
        ("INP", input_available, Color::Green),
    ];
    let content_width =
        INDICATOR_COUNT * INDICATOR_WIDTH + indicators.len().saturating_sub(1) * INDICATOR_GAP;
    let left_padding = usize::from(width).saturating_sub(content_width) / 2;
    let right_padding = usize::from(width)
        .saturating_sub(content_width)
        .saturating_sub(left_padding);
    let black = Style::default().bg(Color::Black);
    let mut spans = vec![Span::styled(" ".repeat(left_padding), black)];
    for (index, (label, ready, loaded_color)) in indicators.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" ".repeat(INDICATOR_GAP), black));
        }
        let lit = indicator_lit(elapsed, index, ready);
        spans.push(Span::styled(
            format!("{label:^width$}", width = INDICATOR_WIDTH),
            Style::default()
                .fg(Color::Black)
                .bg(if lit { loaded_color } else { Color::Red })
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(" ".repeat(right_padding), black));
    Spans::from(spans)
}

pub fn draw<B: Backend>(
    frame: &mut Frame<B>,
    elapsed: Duration,
    input_available: bool,
    controller_checked: bool,
    expected_midi: Option<&str>,
    build_badge: &str,
) {
    let area = frame.size();
    frame.render_widget(Clear, area);

    if let Some(title_area) = rows_from_top(area, 4, 1) {
        frame.render_widget(
            Paragraph::new(title(elapsed)).alignment(Alignment::Center),
            title_area,
        );
    }

    let meter_width = area.width.saturating_sub(2);
    if let Some(mut meter) = rows_from_top(area, 11, 2) {
        meter.x = meter.x.saturating_add(1);
        meter.width = meter.width.saturating_sub(2);
        frame.render_widget(
            Paragraph::new(thick_meter('L', meter_width, elapsed)),
            meter,
        );
    }
    if let Some(mut meter) = rows_from_top(area, 8, 2) {
        meter.x = meter.x.saturating_add(1);
        meter.width = meter.width.saturating_sub(2);
        frame.render_widget(
            Paragraph::new(thick_meter('R', meter_width, elapsed)),
            meter,
        );
    }

    if let Some(indicator_area) = rows_from_top(area, 0, 1) {
        frame.render_widget(
            Paragraph::new(indicator_row(
                area.width,
                elapsed,
                input_available,
                controller_checked,
                build_badge,
            )),
            indicator_area,
        );
    }

    if elapsed >= MINIMUM_VISIBLE && !input_available {
        if let Some(recovery_area) = rows_from_top(area, 1, 1) {
            frame.render_widget(
                Paragraph::new("CONNECT KEYBOARD OR MIDI INPUT")
                    .alignment(Alignment::Center)
                    .style(
                        Style::default()
                            .fg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                recovery_area,
            );
        }
        if let (Some(expected), Some(expected_area)) = (
            expected_midi.filter(|name| !name.trim().is_empty()),
            rows_from_top(area, 2, 1),
        ) {
            frame.render_widget(
                Paragraph::new(format!("WAITING FOR {expected}"))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray)),
                expected_area,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn render(elapsed: Duration, input_available: bool) -> Buffer {
        render_build(elapsed, input_available, "DEV")
    }

    fn render_build(elapsed: Duration, input_available: bool, build_badge: &str) -> Buffer {
        let backend = TestBackend::new(40, 13);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    elapsed,
                    input_available,
                    true,
                    Some("Stage Keyboard"),
                    build_badge,
                )
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buffer: &Buffer) -> String {
        buffer
            .content
            .iter()
            .map(|cell| cell.symbol.as_str())
            .collect()
    }

    #[test]
    fn keyboard_and_midi_are_equal_qualified_inputs() {
        assert!(qualified_input_available(true, false));
        assert!(qualified_input_available(false, true));
        assert!(qualified_input_available(true, true));
        assert!(!qualified_input_available(false, false));
    }

    #[test]
    fn splash_observes_minimum_time_but_only_waits_without_any_input() {
        assert!(waiting_for_input(Duration::ZERO, true, false));
        assert!(!waiting_for_input(MINIMUM_VISIBLE, true, false));
        assert!(!waiting_for_input(MINIMUM_VISIBLE, false, true));
        assert!(waiting_for_input(MINIMUM_VISIBLE, false, false));
    }

    #[test]
    fn splash_renders_requested_top_origin_rows_at_40x13() {
        let buffer = render(Duration::from_millis(2_750), true);
        let output = text(&buffer);
        assert!(output.contains("shr - daw"));
        assert!(output.contains("L ["));
        assert!(output.contains("R ["));

        for rows in [8..10, 11..13] {
            for y in rows {
                let symbols = (0..40)
                    .map(|x| buffer.get(x, y).symbol.as_str())
                    .collect::<String>();
                assert!(symbols.contains('●'));
                assert!(!symbols.contains('█'));
                assert!(!symbols.contains('│'));
            }
        }
        for x in 4..38 {
            assert_eq!(buffer.get(x, 8).symbol, buffer.get(x, 11).symbol);
            assert_eq!(buffer.get(x, 8).fg, buffer.get(x, 11).fg);
        }
        for y in [1, 2, 3, 5, 6, 7, 10] {
            assert!((0..40).all(|x| buffer.get(x, y).symbol == " "));
        }
    }

    #[test]
    fn title_animates_one_bright_glyph_at_a_time() {
        let first = render(Duration::ZERO, true);
        let next = render(TITLE_STEP, true);
        let title_cells = |buffer: &Buffer| {
            (15..24)
                .filter(|x| buffer.get(*x, 4).fg == Color::LightCyan)
                .collect::<Vec<_>>()
        };
        assert_eq!(title_cells(&first), vec![16]);
        assert_eq!(title_cells(&next), vec![17]);
    }

    #[test]
    fn status_boxes_are_exactly_spaced_and_finish_green() {
        let off = render(Duration::ZERO, true);
        let debug = render(INDICATOR_SWEEP, true);
        let release = render_build(INDICATOR_SWEEP, true, "REL");
        let red = (0..40).filter(|x| off.get(*x, 0).bg == Color::Red).count();
        let green = (0..40)
            .filter(|x| debug.get(*x, 0).bg == Color::Green)
            .count();
        assert_eq!(red, 30);
        assert_eq!(green, 25);
        assert!((0..5).all(|x| debug.get(x, 0).bg == Color::LightBlue));
        assert!((0..40).all(|x| debug.get(x, 0).bg != Color::Red));
        assert!((0..5).all(|x| release.get(x, 0).bg == Color::Green));
        assert!((0..40).all(|x| release.get(x, 0).bg != Color::Red));
        for x in [5, 6, 12, 13, 19, 20, 26, 27, 33, 34] {
            assert_eq!(debug.get(x, 0).bg, Color::Black);
        }
        let debug_labels = (0..40)
            .map(|x| debug.get(x, 0).symbol.as_str())
            .collect::<String>();
        let release_labels = (0..40)
            .map(|x| release.get(x, 0).symbol.as_str())
            .collect::<String>();
        assert_eq!(debug_labels, " DEV    CFG    SND    TTY    CTL    INP ");
        assert_eq!(release_labels, " REL    CFG    SND    TTY    CTL    INP ");
    }

    #[test]
    fn splash_names_missing_input_only_after_the_normal_sweep() {
        let waiting_buffer = render(MINIMUM_VISIBLE, false);
        let waiting = text(&waiting_buffer);
        assert!(waiting.contains("CONNECT KEYBOARD OR MIDI INPUT"));
        assert!(waiting.contains("WAITING FOR Stage Keyboard"));

        let loading = text(&render(MINIMUM_VISIBLE, true));
        assert!(!loading.contains("WAITING FOR"));
        assert!((35..40).all(|x| waiting_buffer.get(x, 0).bg == Color::Red));
    }
}
