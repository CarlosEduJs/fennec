use fncc::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[derive(Default)]
struct StylesState {
    count: i32,
}

#[fncc::command]
fn handle_click(state: &mut StylesState, cx: &mut Context<StylesState>) {
    state.count += 1;
    println!("clicked: {}", state.count);
    cx.notify();
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(500.), px(350.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| StylesState::default()),
        )
        .unwrap();
    });
}
