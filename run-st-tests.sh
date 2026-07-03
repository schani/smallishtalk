#!/bin/sh
# Run the Phase-2 SUnit suites under GNU Smalltalk (SPEC §20 Phase 2).
# Usage: ./run-st-tests.sh [extra .st test files...]
set -e
cd "$(dirname "$0")"

SOURCES="
st/tests/Prelude.st
st/compiler/Compat.st
st/compiler/Treaty.st
st/compiler/Platform.st
st/compiler/AST.st
st/compiler/Lexer.st
st/compiler/Parser.st
st/compiler/ChunkReader.st
st/compiler/CodeGen.st
st/compiler/Encoder.st
st/compiler/ImageWriter.st
st/compiler/Compiler.st
st/jit/AMD64Assembler.st
st/jit/AMD64Disassembler.st
st/jit/AMD64Goldens.st
st/jit/AMD64MacroAssembler.st
st/jit/MethodCompiler.st
"

TESTS="
st/tests/LexerTests.st
st/tests/ParserTests.st
st/tests/ChunkReaderTests.st
st/tests/CodeGenTests.st
st/tests/CaptureTests.st
st/tests/EncoderTests.st
st/tests/ImageWriterTests.st
st/tests/AMD64AssemblerTests.st
st/tests/MethodCompilerTests.st
"

FILES=""
for f in $SOURCES $TESTS "$@"; do
    [ -f "$f" ] && FILES="$FILES $f"
done

OUT=$(gst -Q $FILES st/tests/RunTests.st 2>&1) || true
echo "$OUT"
echo "$OUT" | grep -q "ALL-TESTS-PASSED"
