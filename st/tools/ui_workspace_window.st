"UI demo (windowed): open a live Workspace in a real window. The startUp
 program of the image built by `make ui-workspace` (scripts/run-ui.sh
 --workspace) — needs the `ui` cargo feature and a display. Type to edit,
 drag to select, then right-click the selection for the operate menu:
 'do it' evaluates it, 'print it' also inserts the result. Close the window
 (or press Escape) to quit."
| d w s |
d := Display width: 640 height: 400.
d cursorVisible: false. "the OS draws the cursor over a real window"
w := Workspace bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 640 y: 400)).
w contents: '3 + 4 * 2'.
s := UISupervisor on: d.
s rootView: w.
s run.
