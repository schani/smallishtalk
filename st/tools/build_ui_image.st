"Build a UI image (UI.md M1): kernel + the L2 graphics layer + a driver
 program -> STIM image. Files are loaded top-to-bottom in dependency order.
 Args: programPath outPath. Run from the project root (paths are cwd-relative,
 like build_selfhost_image.st)."
| b |
b := StImageBuilder new.
b fileInFile: 'st/kernel/kernel.st'.
"The compiler, filed into the image so the browser can compile methods live
 (UI.md §9.3) — parser + codegen + encoder + Treaty (no image-writer needed)."
b fileInFile: 'st/selfhost/PlatformImage.st'.
b fileInFile: 'st/compiler/Treaty.st'.
b fileInFile: 'st/compiler/AST.st'.
b fileInFile: 'st/compiler/Lexer.st'.
b fileInFile: 'st/compiler/Parser.st'.
b fileInFile: 'st/compiler/ChunkReader.st'.
b fileInFile: 'st/compiler/CodeGen.st'.
b fileInFile: 'st/compiler/Encoder.st'.
b fileInFile: 'st/ui/kernel/RuntimeCompiler.st'.
b fileInFile: 'st/ui/kernel/Reflection.st'.
b fileInFile: 'st/ui/gfx/Point.st'.
b fileInFile: 'st/ui/gfx/Rectangle.st'.
b fileInFile: 'st/ui/gfx/Form.st'.
b fileInFile: 'st/ui/gfx/BitBlt.st'.
b fileInFile: 'st/ui/gfx/Pen.st'.
b fileInFile: 'st/ui/gfx/StrikeFont.st'.
b fileInFile: 'st/ui/gfx/DefaultFont.st'.
b fileInFile: 'st/ui/gfx/Canvas.st'.
b fileInFile: 'st/ui/gfx/Event.st'.
b fileInFile: 'st/ui/testing/FrameProbe.st'.
b fileInFile: 'st/ui/wm/Viewer.st'.
b fileInFile: 'st/ui/wm/Track.st'.
b fileInFile: 'st/ui/wm/Display.st'.
b fileInFile: 'st/ui/wm/UISupervisor.st'.
b fileInFile: 'st/ui/widgets/View.st'.
b fileInFile: 'st/ui/widgets/Controller.st'.
b fileInFile: 'st/ui/widgets/ListPane.st'.
b fileInFile: 'st/ui/widgets/TextPane.st'.
b fileInFile: 'st/ui/widgets/PanedView.st'.
b fileInFile: 'st/ui/widgets/MenuPane.st'.
b fileInFile: 'st/ui/widgets/ScrollBar.st'.
b fileInFile: 'st/ui/apps/ClassBrowser.st'.
b fileInFile: 'st/ui/apps/Workspace.st'.
b programSource: (Platform readFile: (Smalltalk getArgv: 1)).
b writeTo: (Smalltalk getArgv: 2).
Transcript showCr: 'IMAGE-WRITTEN'.
