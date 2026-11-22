# Perspective Warp V3.2 — published contribution and validation

Recorded 22 November 2026.

The three contributions are published as open PhotoCraft pull requests. This report records the current code and validation results together. Their existing PR descriptions were not refreshed: the connected GitHub integration rejected both description and conversation-comment writes with HTTP 403.

## Review entry points

| Contribution | Upstream pull request | Published and independently compiled head |
| --- | --- | --- |
| Global homography core and headless command | [#16359](https://github.com/storytold/photocraft/pull/16359) | [`fbba73d9bd57a0e1ddfb27544c0299235a43493c`](https://github.com/Darmonia/photocraft/commit/fbba73d9bd57a0e1ddfb27544c0299235a43493c) |
| Legacy multi-plane compatibility | [#16380](https://github.com/storytold/photocraft/pull/16380) | [`71538720151e476910511f3da101b055377c2ae0`](https://github.com/Darmonia/photocraft/commit/71538720151e476910511f3da101b055377c2ae0) |
| Independent-guide editor, including its core dependency | [#16378](https://github.com/storytold/photocraft/pull/16378) | [`a72a070da8dc619540af4775a89cb9b8ad63994c`](https://github.com/Darmonia/photocraft/commit/a72a070da8dc619540af4775a89cb9b8ad63994c) |

**Combined tested source:** [`67e2aeb9b843a83fc10602a363b36f7b3ef2f5ac`](https://github.com/Darmonia/photocraft/tree/67e2aeb9b843a83fc10602a363b36f7b3ef2f5ac), Git tree `dc65055ca29dbce72a238784841872138facf54c`.

The integration contains all three published heads as ancestors. Its tracked files were compared with the tested sources after qualification and after the separate branch checks. This report is added in a documentation-only successor; the tested program files remain unchanged.

The UI branch includes the final core through a normal merge, so its checkout builds independently. Review its [UI-only changes relative to the included core](https://github.com/Darmonia/photocraft/compare/fbba73d9bd57a0e1ddfb27544c0299235a43493c...a72a070da8dc619540af4775a89cb9b8ad63994c). The legacy contribution remains independent of that branch.

## Corrections delivered

- **Core:** enforce the configurable byte and image-format memory allowance, including added alpha, on the exact rendered footprint before rendering or returning `solveOnly`. Preserve allocation-free identity behavior and existing Free Transform admission.
- **Legacy:** validate every equally close gap endpoint, including partial parallel-face overlap; preserve gap ordering after rotations and reflections. Monotonicity follows the destination chord, whose orientation must agree with both local Jacobians. The earlier valid projective-horizon neighbour cases remain accepted. Invalid saved Perspective Warp settings now report a rendering error while retaining their settings and cached pixels; unknown filters retain their previous fall-through behavior.
- **UI:** retain per-canvas identity meshes during Layout guide drags. Refuse Place Embedded/Linked and new native/web drops before decoding or starting mutation jobs when they target the document owned by the global editor. Existing queued drops wait, and other documents retain their own queues and ordering.
- **Integration and quality:** retain an independently compilable UI checkout with its exact core dependency. Resolve the observed Clippy findings without suppressing lints or weakening tests. The final floating-point comparisons preserve NaN, infinity and zero handling.

Fourteen additional tests cover these corrections and the combined legacy-then-global Undo/Redo sequence. The six defect classes were reproduced as failing runtime assertions before their corrections.

## Observed validation

Environment: Ubuntu 24.04 under WSL2, Rust 1.100.0, locked dependencies. Printing was disabled through the same environment controls used by the project CI.

| Runtime coverage | Distinct passing cases | Successful executions |
| --- | ---: | ---: |
| Perspective algorithms and Bicubic reduction regressions | 68 | 68 |
| Global/legacy commands and transformation admission | 43 | 43 |
| Guide editors, canvas routing, preview fallback and Pattern Preview | 89 | 90 |
| Adversarial command parameters (`panic_hunt`) | 1 | 1 |
| **Total** | **201** | **202** |

The focused feature suite contains **200 distinct cases with 201 passing executions**; Pattern Preview is also exercised through a second filter. The adversarial-command test is additional. No executed test failed or was ignored.

Fresh checks on the final program source passed:

- Clippy for `photocraft-algo`, `photocraft-engine` and `photocraft-ui-egui`, with `--all-targets -- -D warnings`.
- `cargo fmt --all --check`.
- `cargo xtask layers` and `cargo xtask panel-commands`.
- `cargo xtask wasm`: 19 crates, plus `photocraft-io --features heif`.
- `git diff --check`.
- A separate `cargo check --locked -p photocraft-algo -p photocraft-engine -p photocraft-ui-egui` at **each of the three exact published heads**.

Catalogue parity is **664/664**. Its successful report from integration `23b4175cdf9d18ff3a4502eb47d8d301832867f6` was retained explicitly: the catalogue, `menus::is_live`, registrations, report generator, dependencies and runtime inputs were unchanged by the final three-file lint refactor. The regenerated parity document was identical. This is a reused, still-applicable result, not a new parity execution at `67e2aeb9b843a83fc10602a363b36f7b3ef2f5ac`.

The three PR heads and their combined integration also merged cleanly in read-only merge-tree checks against upstream main `3ad5e089787dc092763669a05185236659d6c06d` at verification.

## Reproduce the focused runtime checks

Run from a checkout of the combined tested source above, with the project's normal build prerequisites:

```bash
export PHOTOCRAFT_NO_PRINT=1
export LPDEST=photocraft-no-such-printer
set -e

for filter in perspective transform::reduction_regressions; do
  cargo test --locked -p photocraft-algo --lib "$filter"
done

for filter in global_perspective_cmds distort_cmds \
  a_gigapixel_layer_transforms_within_memory_usage \
  a_scale_past_the_pixel_budget_is_refused_and_changes_nothing \
  huge_transform_scales_exceed_the_production_budget_before_clipping; do
  cargo test --locked -p photocraft-engine --lib "$filter"
done

for filter in perspective_ui canvas::tools::global_warp \
  foreign_perspective_session_passes_pointer_to_active_document \
  global_warp_fallback_keeps_the_source_pixels pattern_preview_is_refused; do
  cargo test --locked -p photocraft-ui-egui --lib "$filter"
done

cargo test --locked -p photocraft-engine --test panic_hunt -- --ignored
```

## Hosted CI and qualification limits

The upstream CI runs for the published commits reported `action_required`, awaiting maintainer approval:

- [Core CI](https://github.com/storytold/photocraft/actions/runs/45030974388)
- [Legacy CI](https://github.com/storytold/photocraft/actions/runs/45030974494)
- [UI CI](https://github.com/storytold/photocraft/actions/runs/45030975207)

An additional manual dispatch of the repository CI on the combined fork branch was rejected with HTTP 422: `Actions has been disabled for this repository`. Readback nevertheless reported repository Actions enabled and the CI workflow active. No setting was changed and no hosted CI success is asserted.

This qualification covers the targeted runtime cases, listed gates and standalone branch compilation. It is not an exhaustive execution of the entire workspace test suite. Visual rendering in a live application/browser and screen-reader behavior were not separately qualified.

The interactive guide preview currently admits compatible 8-bit RGB, visible topmost raster layers with Normal blending and an identity display transform. Unsupported selections, masks, locks, active effects and incompatible compositing/display transforms are refused. Enter applies, Escape cancels and Delete removes the selected guide; full keyboard-only guide geometry editing remains outside the implemented UI scope.
