//! Runtime state and navigation logic for Native File-Based Routing (NFBR).

/// A generic stack-based router for navigating between screens/routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Router<R> {
    stack: Vec<R>,
}

impl<R: Clone> Router<R> {
    /// Create a new router initialized with an initial route.
    pub fn new(initial: R) -> Self {
        Self { stack: vec![initial] }
    }

    /// Return a reference to the currently active route.
    pub fn current(&self) -> &R {
        self.stack.last().expect("router stack must never be empty")
    }

    /// Push a new route onto the navigation stack.
    pub fn push(&mut self, route: R) {
        self.stack.push(route);
    }

    /// Pop the current route off the stack, returning to the previous route.
    /// Returns `true` if a route was popped, or `false` if already at the root route.
    pub fn pop(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            true
        } else {
            false
        }
    }

    /// Replace the top route on the stack with a new route.
    pub fn replace(&mut self, route: R) {
        *self.stack.last_mut().expect("router stack must never be empty") = route;
    }

    /// Get the current navigation stack depth.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Returns `true` if the navigation stack is empty.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Get a slice of all routes in the navigation history stack.
    pub fn stack(&self) -> &[R] {
        &self.stack
    }

    /// Navigate to a route parsed from a deep link URI string.
    /// Returns `true` if the URI parsed successfully into a route and was pushed onto the stack.
    pub fn push_uri<F>(&mut self, uri: &str, parser: F) -> bool
    where
        F: FnOnce(&str) -> Option<R>,
    {
        if let Some(route) = parser(uri) {
            self.push(route);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestRoute {
        Home,
        Settings,
        User { id: String },
    }

    impl TestRoute {
        fn from_uri(uri: &str) -> Option<Self> {
            match uri {
                "/" | "/home" => Some(TestRoute::Home),
                "/settings" => Some(TestRoute::Settings),
                s if s.starts_with("/user/") => Some(TestRoute::User {
                    id: s.trim_start_matches("/user/").to_string(),
                }),
                _ => None,
            }
        }
    }

    #[test]
    fn test_router_initial_state() {
        let router = Router::new(TestRoute::Home);
        assert_eq!(router.current(), &TestRoute::Home);
        assert_eq!(router.len(), 1);
        assert!(!router.is_empty());
    }

    #[test]
    fn test_router_push() {
        let mut router = Router::new(TestRoute::Home);
        router.push(TestRoute::Settings);
        assert_eq!(router.current(), &TestRoute::Settings);
        assert_eq!(router.len(), 2);
    }

    #[test]
    fn test_router_pop() {
        let mut router = Router::new(TestRoute::Home);
        router.push(TestRoute::Settings);

        assert!(router.pop());
        assert_eq!(router.current(), &TestRoute::Home);
        assert_eq!(router.len(), 1);

        // Cannot pop root route
        assert!(!router.pop());
        assert_eq!(router.current(), &TestRoute::Home);
    }

    #[test]
    fn test_router_replace() {
        let mut router = Router::new(TestRoute::Home);
        router.replace(TestRoute::Settings);
        assert_eq!(router.current(), &TestRoute::Settings);
        assert_eq!(router.len(), 1);
    }

    #[test]
    fn test_router_push_uri() {
        let mut router = Router::new(TestRoute::Home);

        assert!(router.push_uri("/settings", TestRoute::from_uri));
        assert_eq!(router.current(), &TestRoute::Settings);

        assert!(router.push_uri("/user/42", TestRoute::from_uri));
        assert_eq!(router.current(), &TestRoute::User { id: "42".to_string() });

        assert!(!router.push_uri("/invalid", TestRoute::from_uri));
        assert_eq!(router.current(), &TestRoute::User { id: "42".to_string() });
    }
}
