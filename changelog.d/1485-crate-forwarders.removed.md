- `openehr-federation` 0.0.57 drops `object::Extra::new` and
  `order::ResultOrder::unordered`, each the same as the type's `Default`.
  A caller writes `Extra::default()` and `ResultOrder::default()` (#1485).
