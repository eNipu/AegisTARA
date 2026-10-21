## Answers

The owner answered the spec's open question on 2026-10-21:

1. **Scope of the 3x budget:** confirmed. "Server memory per open project ≤ 3x the serialized size" limits the **open project**, meaning the heap still held after `Project::load` returns ("kept"). This job gates kept ≤ 3x for every shape. The brief loading peak (3.53x for damage-scenarios-only over a 32-category methodology, from dense impact slots) stays gated and documented as it is, and is not in scope.

All other decisions stay as the spec settled them.
