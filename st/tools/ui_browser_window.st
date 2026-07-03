"UI demo (windowed): open a live, click-navigable Class Browser in a real
 window. The startUp program of the image built by `make ui-window`
 (scripts/run-ui.sh --window) — needs the `ui` cargo feature and a display.
 Click the category / class / protocol / selector lists to navigate; close the
 window (or press Escape) to quit."
| d b s |
Smalltalk organization classify: (Smalltalk classNamed: 'Array') under: 'Collections'.
Smalltalk organization classify: (Smalltalk classNamed: 'String') under: 'Collections'.
Smalltalk organization classify: (Smalltalk classNamed: 'OrderedCollection') under: 'Collections'.
Smalltalk organization classify: (Smalltalk classNamed: 'Dictionary') under: 'Collections'.
d := Display width: 800 height: 520.
d cursorVisible: false. "the OS draws the cursor over a real window"
b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 800 y: 520)).
b selectCategoryNamed: 'Kernel'.
s := UISupervisor on: d.
s rootView: b.
s run.
