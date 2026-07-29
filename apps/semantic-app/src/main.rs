use fncc::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[derive(Default)]
struct CounterState {
    count: i32,
}

#[fncc::command]
fn increment(state: &mut CounterState, cx: &mut Context<CounterState>) {
    state.count += 1;
    println!("count: {}", state.count);
    cx.notify();
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(400.), px(200.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| CounterState::default()),
        )
        .unwrap();
    });
}
