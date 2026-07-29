use fncc::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[derive(Default)]
struct AppState {
    count: i32,
}

#[fncc::command]
fn handle_click(state: &mut AppState, cx: &mut Context<AppState>) {
    state.count += 1;
    cx.notify();
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(400.), px(300.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| AppState::default()),
        )
        .unwrap();
    });
}
