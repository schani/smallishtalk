"UI demo (headless): populate and render a Class Browser, then save it as a PNG.
 The startUp program of the image built by `make ui` / scripts/run-ui.sh."
| b f |
Smalltalk organization classify: (Smalltalk classNamed: 'Array') under: 'Collections'.
Smalltalk organization classify: (Smalltalk classNamed: 'String') under: 'Collections'.
Smalltalk organization classify: (Smalltalk classNamed: 'OrderedCollection') under: 'Collections'.
Smalltalk organization classify: (Smalltalk classNamed: 'Dictionary') under: 'Collections'.
b := ClassBrowser bounds: (Rectangle origin: (Point x: 0 y: 0) corner: (Point x: 640 y: 400)).
b selectCategoryNamed: 'Collections'.
b selectClassNamed: 'OrderedCollection'.
b selectProtocolNamed: 'unclassified'.
b selectSelectorNamed: 'add:'.
f := Form width: 640 height: 400.
b displayOn: (Canvas on: f).
f saveTo: 'ui-screenshot.png'.
Transcript showCr: 'UI-SCREENSHOT-WRITTEN ui-screenshot.png'.
