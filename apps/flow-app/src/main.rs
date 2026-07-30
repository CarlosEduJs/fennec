use fncc::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[derive(Default)]
struct FlowState {
    count: i32,
    items: Vec<String>,
    show_details: bool,
}

#[fncc::command]
fn increment(state: &mut FlowState, cx: &mut Context<FlowState>) {
    state.count += 1;
    state.items.push(format!("Item {}", state.count));
    println!("Incremented count to {}", state.count);
    cx.notify();
}

#[fncc::command]
fn toggle_details(state: &mut FlowState, cx: &mut Context<FlowState>) {
    state.show_details = !state.show_details;
    println!("Toggled details to {}", state.show_details);
    cx.notify();
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(500.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| FlowState::default()),
        )
        .unwrap();
    });
}
