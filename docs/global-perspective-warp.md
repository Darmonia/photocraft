# Global Perspective Warp - solver and engine command

This first independent contribution adds only the solver and headless engine
command edit.perspectiveWarpGlobal; the guide editor UI is a separate PR.

Independent door/window quadrilaterals constrain ONE global homography of
the entire active raster photo layer. No piecewise patch warps, folds or
stitching are performed. Incompatible guides are reported. The transform is
validated across the entire source domain (finite denominator, positive
Jacobian and bounded raster allocation). Physical aspect cannot be inferred
without metric information; options.aspect is a global manual correction.

Engine command: edit.perspectiveWarpGlobal
Example params: {"guides":[{"id":1,"corners":[[25,20],[100,16],[109,105],[21,110]]}],
"options":{"fitCanvas":true,"aspect":1},"solveOnly":true}

Guide IDs must be unique, corners ordered TL/TR/BR/BL, up to 128 guides,
weights 0.05..20, aspect 0.25..4, maximum 32768 pixels per image side,
and maximum final allocation 67,108,864 output pixels. Invalid geometry,
selection, masks, locked or non-raster layers are explicitly refused.
Optional layer, document, interpolation and solveOnly are supported.
solveOnly returns diagnostics without document/history mutation.
An effective warp commits once and supports Undo/Redo. Rejection is atomic.

Tests: crates/algo/src/global_perspective.rs and
crates/engine/src/global_perspective_cmds.rs.

Provenance: original full V3.2 source at SHA
8946cc97cc3fb37ad967064d053c40c79879bfbb in PR #5651.
Prior portable Windows acceptance applies only to that exact older source;
this split branch requires its own build/tests/CI before integration.

## Edit guides interactive UI

Open Edit > Global Perspective Warp. Guides are independent TL/TR/BR/BL quadrilaterals. Drag their corners or whole edges, correct options, rectify globally and press Enter to commit, or Esc to cancel without pixel changes. Preview and commit use the same global map. A stale document or invalid geometry is refused, retaining the editable session.

Interactive control transport example:


```json
["ui.menu.invoke",{"id":"edit.perspectiveWarpGlobal"}]
["ui.menu.invoke",{"id":"edit.perspectiveWarpGlobal","params":{"ui":{"guides":[{"id":1,"corners":[[25,20],[100,16],[109,105],[21,110]]}],"solve":true}}}]
["ui.inspect",{}]
["ui.menu.invoke",{"id":"edit.perspectiveWarpGlobal","params":{"ui":{"commit":true}}}]
```

Interactive `params` may contain ONLY `ui`; use `engine.execute` for direct engine data. UI fields: `guides` (replace complete list), `options` (replace options object with documented defaults), `mode` (`layout|result`), `selectedGuide` (existing ID), `solve`, `commit`, `cancel`. `result` requests a current solve. `layout` changes the view without changing geometry. Replacement data and a requested solve are validated atomically before changing the old session. Optional `null` guide/options/selection/mode fields mean omitted; booleans must be booleans. Commit and cancel must be standalone actions, not combined with replacements or solve. Commit requires a solved, unchanged source session. An absent/invalid solution is never interpreted as success.

`ui.inspect` exposes `distort.globalPerspective` (alongside legacy `distort.perspective`). It includes `version`, `document`, `layer`, `mode`, source `guides`, `selectedGuide`, `options`, `solved`, `dirty`, `scope`, and `solution`. The solution includes:

- `model: singleGlobalHomography`, row-major `matrix[9]`, `sourceBounds`, `outputBounds`;
- `usedGuideIds`, `rejectedGuideIds`, and per-guide `enabled`, `used`, `maxErrorDeg`, `transformedCorners`;
- `maxUsedErrorDeg`, `minDenominator`, `minJacobian`, `aspectAmbiguous`, and `warnings`.

Do not equate `solved` with human visual acceptance, metric calibration or compatibility of rejected guides. Both UI and engine call the same solver.


### Precision guide editing


- **Ctrl + drag a corner**: constrain the movement to the horizontal or vertical axis with the strongest initial drag motion. Releasing Ctrl restores free movement. The other corners remain stationary.
- **Hover an edge**: the cursor changes to a move icon. Drag the edge to translate **both of its endpoint corners together**, without moving the opposite side. Ctrl constrains the edge translation to one axis as well.
- Corner handles take priority over edges at intersections. These edits only update guide geometry; global rectification is recomputed when **Rectify globally** is pressed. Result preview does not permit local warping.

- **Beyond the photo**: drag a guide corner or edge into the pasteboard outside the image to align extended architectural lines. The guide may extend beyond the pixels; only the photo is transformed. Safety validation still rejects self-intersections, extreme coordinates, or a projective map that becomes unsafe.


### V3.2 - local perspective-axis lock

In **Edit guides**, hold **Shift while dragging a corner** to move it along either
of the two adjoining perspective directions. The two lines are derived from the
quad **at pointer down** and remain fixed throughout that drag. The first deliberate
mouse movement chooses the nearest (unoriented) line; later diagonal noise does not
switch it. The signed projection permits extending a corner away from its neighbour,
including beyond the photo border.

The activation dead zone is two screen points at the initial zoom. A theme-coloured
rail displays the locked line. Releasing Shift restores free dragging; pressing it
again reselects an axis from the same original quad. Holding **Ctrl + Shift** gives
**Ctrl priority**, retaining the image-horizontal/image-vertical behavior. The corner
grab offset is preserved, and the final pointer-up sample uses the same constraint.

This addition is intentionally for **corners**. Edge dragging still translates both
endpoints together, with its existing Ctrl constraint; Shift does not create a new
edge-warp model. Shift-click away from handles can still create a guide inside another.
All convexity, finite-coordinate, safe-domain and stale-document guards remain active.
No local image pieces are introduced: the final raster operation is still one global
homography applied to the active photo layer.

For agents, existing `ui.pointer` events with `modifiers: {"shift": true}` exercise
the same path as the mouse. `ui.inspect` / global-session descriptions add
`interactionRevision: "3.2"` and `localAxisConstraint` (null unless a local axis is
locked; otherwise `{anchor, direction, space: "guide", frozen: true}`). Coordinates
remain document coordinates. Constraint diagnostics are transient, not a stored
pixel transform. `guides`, `options`, solve, commit and cancel contracts are unchanged.

Example for a source corner at [60, 45] with adjoining top edge toward [230, 25]:

```json
{"method":"ui.pointer","params":{"modifiers":{"shift":true},"events":[{"kind":"down","x":60,"y":45},{"kind":"move","x":100,"y":47}]}}
```

The corner moves along the original [170, -20] direction, not the image X axis.
Send an `up` event with Shift still held to finish that same constrained gesture.
