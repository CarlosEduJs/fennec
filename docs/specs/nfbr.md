# Native File-Based Routing (NFBR) Specification
Version: v0.1
Status: Draft

---

# Overview

FNCC provides a compile-time Native File-Based Routing (NFBR) system, designed specifically for GPUI desktop applications.

Unlike web routers, NFBR is built around native navigation concepts: navigation stacks (push/pop), deep link URI schemes, window management, and strongly-typed in-memory payloads. 

Routes are discovered by scanning the project directory and generating a routing table and a deep link schema during compilation. No runtime route registration is required.

---

# Layout Routes

A directory may define a layout by providing a `layout.fui` file.

A layout wraps every route contained in its directory and all nested
subdirectories unless another layout overrides it. This is ideal for native Sidebar Navigation, Split Views, or Tab Bars.

Layouts render child routes through `<RouterOutlet />`.

Example:

```
routes/
├── layout.fui
├── index.fui
├── settings.fui
└── dashboard/
    ├── layout.fui
    ├── index.fui
    └── analytics.fui
```

Produces:

```
layout.fui
└── index.fui

layout.fui
└── settings.fui

layout.fui
└── dashboard/layout.fui
    └── dashboard/index.fui

layout.fui
└── dashboard/layout.fui
    └── dashboard/analytics.fui
```

---

# Route Root

The routing root is:

```
src/ui/routes/
```

Every `.fui` file inside this directory represents a route screen.

Example:

```
src/ui/routes/
├── index.fui
├── settings.fui
└── profile.fui
```

Produces:

```
/
/settings
/profile
```

---

# Index Routes

A file named `index.fui` represents the directory itself.

Example:

```
routes/
├── index.fui
└── dashboard/
    └── index.fui
```

Produces:

```
/
/dashboard
```

---

# Nested Routes

Directories represent nested route segments.

Example:

```
routes/
└── users/
    ├── index.fui
    └── settings.fui
```

Produces:

```
/users
/users/settings
```

---

# Route Groups (Pathless Layouts)

Directories wrapped in parentheses `()` organize routes and share layouts without affecting the deep link paths.

Example:

```
routes/
├── (app)/
│   ├── layout.fui
│   └── dashboard.fui
└── (auth)/
    ├── layout.fui
    └── login.fui
```

Produces deep links:

```
/dashboard (wrapped by (app)/layout.fui)
/login (wrapped by (auth)/layout.fui)
```

---

# Dynamic Routes

Files surrounded by brackets represent route parameters, which are automatically typed by the compiler.

Example:

```
routes/
└── users/
    └── [id].fui
```

Produces:

```
/users/:id
```

Inside the route, the parameter is accessed directly as `{id}`, matching the generated stateless function signature (e.g. `render_users_id(id: &str)`).

---

# Typed Payloads

> [!NOTE]
> This feature is planned for a future release and is **not implemented** in v0.1.

Unlike the web where data must be serialized to a URL string, native routing is planned to allow passing complex in-memory Rust structs directly between screens.

---

# Named Routes & Deep Linking

Every route automatically receives a generated enum variant.

Example:

```
routes/
└── users/
    └── [id].fui
```

Generated enum variant:

```rust
Route::UsersId { id: String }
```

Additionally, the NFBR tree automatically generates the application's Deep Link Schema (URI scheme). 
Opening `myapp://users/123` from the Operating System will automatically route to the corresponding screen and parse `123` into the `id` parameter.

---

# Stack Navigation

Navigation is native and stack-based, accessed through the router.

Example:

```rust
// Pushes a new screen onto the stack, allowing the user to go back
router.push(Route::Settings)

// Replaces the current screen (no history left for the replaced screen)
router.replace(Route::Dashboard)

// Pops the current screen off the stack, returning to the previous one
router.pop()
```

The generated API is fully type-safe.

---

# Window Management

> [!NOTE]
> Multi-window routing is planned for a future release and is **not implemented** in v0.1.

Since GPUI supports multi-window applications, the router is planned to explicitly support opening routes in new native OS windows.

---

# RouterOutlet

RouterOutlet renders the currently matched child route.

Example:

```xml
<Stack>
    <Sidebar />
    <RouterOutlet />
</Stack>
```

Nested routes render inside the closest RouterOutlet.

---

# Fallback Route (Unhandled Links)

A special file

```
routes/fallback.fui
```

defines the application's Fallback screen. In a desktop context, this is triggered when the app receives an invalid Deep Link from the OS or a navigation fails.

If omitted, unresolved links return `None` and no default fallback handler is generated.

---

# Route Resolution Priority

Resolution order:

1. Static routes
2. Dynamic routes
3. Fallback

Example:

```
/users/settings
/users/:id
```

The static route always wins.

---

# Compile-Time Validation

The compiler validates:

- duplicated routes
- duplicated route names
- invalid dynamic parameter syntax
- invalid route tree
- unresolved navigation targets

Compilation fails on errors.

---

# LSP Metadata

Compilers must expose route metadata.

Metadata includes:

- path (deep link)
- route name (enum variant)
- source file
- parameters & payloads
- nesting
- parent route

This metadata is intended for:

- autocomplete
- rename
- hover
- go-to-definition
- diagnostics

---

# Future Extensions

The following features are intentionally outside v0.1:

- Route Guards (Interceptors)
- Middleware
- Route Lifecycle (OnEnter, OnLeave)
- Route Transitions (Push/Pop Animations)
- Route Cache (Keep-alive screens)
- Typed Payloads (Passing structs directly between screens)
- Window Management (Opening routes in new GPUI windows)

These may be added without changing the routing syntax.