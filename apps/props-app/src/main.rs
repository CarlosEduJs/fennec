use fncc::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[derive(Default)]
struct AppState {
    clicks: i32,
}

#[derive(fncc::Props)]
pub struct HeaderProps {
    pub title: String,
    pub subtitle: String,
}

#[derive(fncc::Props)]
pub struct CardProps {
    pub heading: String,
    pub body: String,
}

#[fncc::command]
fn click_card(state: &mut AppState, cx: &mut Context<AppState>) {
    state.clicks += 1;
    cx.notify();
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(520.), px(440.)), cx);
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
