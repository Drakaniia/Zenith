use crate::models::WindowState;
use tauri::WebviewWindow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowControlOperation {
    Unminimize,
    Maximize,
    Unmaximize,
}

#[tauri::command]
pub fn get_window_state(window: WebviewWindow) -> Result<WindowState, String> {
    current_window_state(&window, "get_window_state")
}

#[tauri::command]
pub fn minimize_window(window: WebviewWindow) -> Result<WindowState, String> {
    window
        .minimize()
        .map_err(|error| format!("minimize_window: {error}"))?;

    current_window_state(&window, "minimize_window")
}

#[tauri::command]
pub fn maximize_window(window: WebviewWindow) -> Result<WindowState, String> {
    let state = current_window_state(&window, "maximize_window")?;

    for operation in maximize_window_actions(state.is_minimized, state.is_maximized) {
        apply_window_operation(&window, operation, "maximize_window")?;
    }

    current_window_state(&window, "maximize_window")
}

#[tauri::command]
pub fn restore_window(window: WebviewWindow) -> Result<WindowState, String> {
    let state = current_window_state(&window, "restore_window")?;

    for operation in restore_window_actions(state.is_minimized, state.is_maximized) {
        apply_window_operation(&window, operation, "restore_window")?;
    }

    current_window_state(&window, "restore_window")
}

#[tauri::command]
pub fn close_window(window: WebviewWindow) -> Result<(), String> {
    window
        .close()
        .map_err(|error| format!("close_window: {error}"))
}

fn current_window_state(window: &WebviewWindow, command: &str) -> Result<WindowState, String> {
    Ok(WindowState {
        is_minimized: window
            .is_minimized()
            .map_err(|error| format!("{command}: {error}"))?,
        is_maximized: window
            .is_maximized()
            .map_err(|error| format!("{command}: {error}"))?,
    })
}

fn maximize_window_actions(is_minimized: bool, is_maximized: bool) -> Vec<WindowControlOperation> {
    let mut actions = Vec::new();

    if is_minimized {
        actions.push(WindowControlOperation::Unminimize);
    }

    if !is_maximized {
        actions.push(WindowControlOperation::Maximize);
    }

    actions
}

fn restore_window_actions(is_minimized: bool, is_maximized: bool) -> Vec<WindowControlOperation> {
    let mut actions = Vec::new();

    if is_minimized {
        actions.push(WindowControlOperation::Unminimize);
    }

    if is_maximized {
        actions.push(WindowControlOperation::Unmaximize);
    }

    actions
}

fn apply_window_operation(
    window: &WebviewWindow,
    operation: WindowControlOperation,
    command: &str,
) -> Result<(), String> {
    match operation {
        WindowControlOperation::Unminimize => window.unminimize(),
        WindowControlOperation::Maximize => window.maximize(),
        WindowControlOperation::Unmaximize => window.unmaximize(),
    }
    .map_err(|error| format!("{command}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{restore_window_actions, WindowControlOperation};

    #[test]
    fn restore_unminimizes_and_unmaximizes_to_return_to_windowed_state() {
        let actions = restore_window_actions(true, true);

        assert_eq!(
            actions,
            vec![
                WindowControlOperation::Unminimize,
                WindowControlOperation::Unmaximize
            ]
        );
    }

    #[test]
    fn restore_unmaximizes_a_maximized_window() {
        let actions = restore_window_actions(false, true);

        assert_eq!(actions, vec![WindowControlOperation::Unmaximize]);
    }

    #[test]
    fn restore_unminimizes_a_minimized_window() {
        let actions = restore_window_actions(true, false);

        assert_eq!(actions, vec![WindowControlOperation::Unminimize]);
    }

    #[test]
    fn restore_does_nothing_for_a_normal_window() {
        let actions = restore_window_actions(false, false);

        assert!(actions.is_empty());
    }
}
