(module
  ;; The host resolves this import only when read_markets was both requested
  ;; in the signed manifest and explicitly granted by the operator.
  (import "aether" "read_markets" (func $read_markets (result i32)))
  (func (export "run") (result i32)
    call $read_markets))
