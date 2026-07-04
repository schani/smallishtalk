//! The Treaty (SPEC.md Appendix A): the binary contract between the VM and
//! the image-side compiler. `treaty.json` is canonical; this file mirrors it
//! and the `treaty_agrees_with_json` test asserts the two never drift.

// --- Tags (A.1) ---
pub const TAG_INT_BIT: u64 = 0x1;
pub const TAG_PTR_MASK: u64 = 0x7;
pub const TAG_PTR: u64 = 0x0;
pub const TAG_FLOAT_IMM: u64 = 0x2; // reserved, never produced in v1
pub const SMALLINT_BITS: u32 = 63;

// --- Object header (A.1) ---
pub const HDR_CLASS_SHIFT: u32 = 42;
pub const HDR_CLASS_BITS: u32 = 22;
pub const HDR_HASH_SHIFT: u32 = 20;
pub const HDR_HASH_BITS: u32 = 22;
pub const HDR_NSLOTS_SHIFT: u32 = 12;
pub const HDR_NSLOTS_BITS: u32 = 8;
pub const HDR_NSLOTS_OVERFLOW: u64 = 255;
pub const HDR_FORMAT_SHIFT: u32 = 8;
pub const HDR_FORMAT_BITS: u32 = 4;
pub const HDR_GC_SHIFT: u32 = 0;
pub const HDR_GC_BITS: u32 = 8;

pub const GC_BIT_MARK: u64 = 1;
pub const GC_BIT_REMEMBERED: u64 = 2;
pub const GC_BIT_PINNED: u64 = 4;
pub const GC_AGE_SHIFT: u32 = 3;
pub const GC_AGE_BITS: u32 = 3;
pub const GC_BIT_IMMUTABLE: u64 = 128; // Treaty: bit 7 of gcBits

// --- Formats (§3) ---
pub const FMT_FIXED: u64 = 0;
pub const FMT_PTRS: u64 = 1;
pub const FMT_BYTES_BASE: u64 = 8; // pad = format - 8 (0..7)

// --- Frame layout (A.1, §7) ---
pub const FRAME_CALLER: usize = 0;
pub const FRAME_RETINFO: usize = 1;
pub const FRAME_METHOD: usize = 2;
pub const FRAME_FLAGS: usize = 3;
pub const FRAME_RECEIVER: usize = 4;
pub const FRAME_FIXED: usize = 5;
pub const FLAG_HANDLER: u64 = 1;
pub const FLAG_ENSURE: u64 = 2;
pub const FLAG_BLOCKCTX: u64 = 4;
/// VM-internal: this block activation was pushed by the unwinder to run an
/// ensure block; on return, unwinding continues using the pending target/value
/// stored in the caller (ensure) frame's reserved slots.
pub const FLAG_UNWINDCONT: u64 = 8;
pub const SERIAL_SHIFT: u32 = 32;
// returnInfo (frame slot 1) packing: dest slot in low 8 bits, resume pc above.
pub const RETINFO_DEST_BITS: u32 = 8;
pub const RETINFO_PC_SHIFT: u32 = 8;

// --- Opcodes (A.2) ---
pub const OP_NOP: u8 = 0x00;
pub const OP_BREAK: u8 = 0x01;
pub const OP_MOVE: u8 = 0x10;
pub const OP_LOADK: u8 = 0x11;
pub const OP_LOADINT: u8 = 0x12;
pub const OP_LOADNIL: u8 = 0x13;
pub const OP_LOADTRUE: u8 = 0x14;
pub const OP_LOADFALSE: u8 = 0x15;
pub const OP_LOADSELF: u8 = 0x16;
pub const OP_GETIVAR: u8 = 0x17;
pub const OP_SETIVAR: u8 = 0x18;
pub const OP_GETBOX: u8 = 0x19;
pub const OP_SETBOX: u8 = 0x1A;
pub const OP_MKBOX: u8 = 0x1B;
pub const OP_SEND: u8 = 0x20;
pub const OP_SENDSUPER: u8 = 0x21;
pub const OP_RET: u8 = 0x22;
pub const OP_RETSELF: u8 = 0x23;
pub const OP_NLR: u8 = 0x24;
pub const OP_PRIM: u8 = 0x25;
pub const OP_MKCLOSURE: u8 = 0x26;
pub const OP_CAPTURE: u8 = 0x27;
pub const OP_JUMP: u8 = 0x28;
pub const OP_JUMPTRUE: u8 = 0x29;
pub const OP_JUMPFALSE: u8 = 0x2A;
pub const OP_ADD: u8 = 0x30;
pub const OP_SUB: u8 = 0x31;
pub const OP_MUL: u8 = 0x32;
pub const OP_DIV: u8 = 0x33;
pub const OP_MOD: u8 = 0x34;
pub const OP_LT: u8 = 0x35;
pub const OP_GT: u8 = 0x36;
pub const OP_LE: u8 = 0x37;
pub const OP_GE: u8 = 0x38;
pub const OP_EQNUM: u8 = 0x39;
pub const OP_AT: u8 = 0x40;
pub const OP_ATPUT: u8 = 0x41;
pub const OP_SIZE: u8 = 0x42;
pub const OP_CLASSOF: u8 = 0x43;
pub const OP_NOT: u8 = 0x44;
pub const OP_IDEQ: u8 = 0x45;

// --- Treaty class indices (A.3) ---
pub const CLASS_OBJECT: u32 = 1;
pub const CLASS_BEHAVIOR: u32 = 2;
pub const CLASS_CLASS: u32 = 3;
pub const CLASS_METACLASS: u32 = 4;
pub const CLASS_UNDEFINED_OBJECT: u32 = 5;
pub const CLASS_TRUE: u32 = 6;
pub const CLASS_FALSE: u32 = 7;
pub const CLASS_SMALLINTEGER: u32 = 8;
pub const CLASS_FLOAT: u32 = 9;
pub const CLASS_CHARACTER: u32 = 10;
pub const CLASS_STRING: u32 = 11;
pub const CLASS_BYTESTRING: u32 = 12;
pub const CLASS_SYMBOL: u32 = 13;
pub const CLASS_LARGE_POSITIVE_INTEGER: u32 = 14;
pub const CLASS_LARGE_NEGATIVE_INTEGER: u32 = 15;
pub const CLASS_ARRAY: u32 = 16;
pub const CLASS_BYTEARRAY: u32 = 17;
pub const CLASS_ORDERED_COLLECTION: u32 = 18;
pub const CLASS_ASSOCIATION: u32 = 19;
pub const CLASS_BOX: u32 = 20;
pub const CLASS_BLOCKCLOSURE: u32 = 21;
pub const CLASS_COMPILEDMETHOD: u32 = 22;
pub const CLASS_COMPILEDBLOCK: u32 = 23;
pub const CLASS_PROCESS: u32 = 24;
pub const CLASS_SEMAPHORE: u32 = 25;
pub const CLASS_METHODDICTIONARY: u32 = 26;
pub const CLASS_PROCESSOR_SCHEDULER: u32 = 27;
pub const CLASS_MESSAGE: u32 = 28;
pub const CLASS_SYSTEM_DICTIONARY: u32 = 29;
pub const CLASS_STACK: u32 = 30;
pub const CLASS_LINKED_LIST: u32 = 31;
pub const FIRST_UNRESERVED_CLASS_INDEX: u32 = 64;

// --- Stack object layout (§7) ---
// Slot 0 holds the owning Process (the GC needs the owner to compute the
// live frame extent); frames start at slot 1, so a callerFrameOffset of 0
// is an unambiguous "base frame" sentinel.
pub const STACK_OWNER: usize = 0;
pub const STACK_FRAMES_BASE: usize = 1;

// --- MethodDictionary layout (§4, §14) ---
// Parallel keys/values Arrays, linear identity scan by the VM.
pub const MDICT_KEYS: usize = 0;
pub const MDICT_VALUES: usize = 1;
pub const MDICT_NUM_VM_SLOTS: usize = 2;

// --- Run-queue linked list (§13) ---
pub const LIST_HEAD: usize = 0;
pub const LIST_TAIL: usize = 1;
pub const LIST_NUM_VM_SLOTS: usize = 2;

// --- Handler / ensure reserved frame slots (§11), relative to the method
// header's handlerSlotBase (a bytecode slot index) ---
pub const HANDLER_SLOT_CLASS: usize = 0;
pub const HANDLER_SLOT_BLOCK: usize = 1;
pub const HANDLER_SLOT_STATE: usize = 2;
pub const HANDLER_STATE_ARMED: i64 = 1;
pub const HANDLER_STATE_IN_PROGRESS: i64 = 2;
pub const ENSURE_SLOT_BLOCK: usize = 0;
pub const ENSURE_SLOT_PENDING_TARGET: usize = 1;
pub const ENSURE_SLOT_PENDING_SERIAL: usize = 2;
pub const ENSURE_SLOT_PENDING_VALUE: usize = 3;

// --- Indices into the specializedSelectors array (A.4 index 7) ---
pub const SPECSEL_PLUS: usize = 0;
pub const SPECSEL_MINUS: usize = 1;
pub const SPECSEL_TIMES: usize = 2;
pub const SPECSEL_INT_DIV: usize = 3;
pub const SPECSEL_MOD: usize = 4;
pub const SPECSEL_LT: usize = 5;
pub const SPECSEL_GT: usize = 6;
pub const SPECSEL_LE: usize = 7;
pub const SPECSEL_GE: usize = 8;
pub const SPECSEL_EQ: usize = 9;
pub const SPECSEL_IDENTICAL: usize = 10;
pub const SPECSEL_AT: usize = 11;
pub const SPECSEL_AT_PUT: usize = 12;
pub const SPECSEL_SIZE: usize = 13;
pub const SPECSEL_CLASS: usize = 14;
pub const SPECSEL_NOT: usize = 15;
pub const SPECSEL_COUNT: usize = 16;

// --- Behavior slots (§4) ---
pub const BEHAVIOR_SUPERCLASS: usize = 0;
pub const BEHAVIOR_METHOD_DICTIONARY: usize = 1;
pub const BEHAVIOR_FORMAT_AND_SLOTS: usize = 2;
pub const BEHAVIOR_CLASS_INDEX: usize = 3;
pub const BEHAVIOR_NUM_VM_SLOTS: usize = 4;
// formatAndSlots (SmallInteger): instance format in high bits, named-slot count low.
pub const FORMAT_AND_SLOTS_FORMAT_SHIFT: u32 = 16;
pub const FORMAT_AND_SLOTS_NSLOTS_MASK: u64 = 0xFFFF;

// --- Process slots (§7) ---
pub const PROCESS_STACK: usize = 0;
pub const PROCESS_FRAME_OFFSET: usize = 1;
pub const PROCESS_PC: usize = 2;
pub const PROCESS_PRIORITY: usize = 3;
pub const PROCESS_NEXT_LINK: usize = 4;
pub const PROCESS_MY_LIST: usize = 5;
pub const PROCESS_SERIAL_COUNTER: usize = 6;
pub const PROCESS_NUM_VM_SLOTS: usize = 7;

// --- Semaphore slots (§13) ---
pub const SEMAPHORE_EXCESS_SIGNALS: usize = 0;
pub const SEMAPHORE_QUEUE_HEAD: usize = 1;
pub const SEMAPHORE_QUEUE_TAIL: usize = 2;
pub const SEMAPHORE_NUM_VM_SLOTS: usize = 3;

// --- ProcessorScheduler slots (§13) ---
pub const SCHEDULER_QUEUES: usize = 0;
pub const SCHEDULER_ACTIVE_PROCESS: usize = 1;
pub const SCHEDULER_NUM_VM_SLOTS: usize = 2;
pub const NUM_PRIORITIES: usize = 8;

// --- CompiledMethod / CompiledBlock slots (§9) ---
pub const METHOD_HEADER: usize = 0;
pub const METHOD_BYTECODES: usize = 1;
pub const METHOD_LITERALS: usize = 2;
pub const METHOD_SEND_SITES: usize = 3;
pub const METHOD_SELECTOR: usize = 4;
pub const METHOD_CLASS: usize = 5;
pub const METHOD_SOURCE_INFO: usize = 6;
pub const METHOD_NUM_SLOTS: usize = 7;

pub const BLOCK_HEADER: usize = 0;
pub const BLOCK_BYTECODES: usize = 1;
pub const BLOCK_LITERALS: usize = 2;
pub const BLOCK_SEND_SITES: usize = 3;
pub const BLOCK_OUTER_METHOD: usize = 4;
pub const BLOCK_INFO: usize = 5;
pub const BLOCK_NUM_SLOTS: usize = 6;

// --- Method header packing (§9) ---
pub const MH_FRAME_SLOTS_SHIFT: u32 = 0;
pub const MH_FRAME_SLOTS_BITS: u32 = 8;
pub const MH_ARGC_SHIFT: u32 = 8;
pub const MH_ARGC_BITS: u32 = 4;
pub const MH_PRIMITIVE_SHIFT: u32 = 12;
pub const MH_PRIMITIVE_BITS: u32 = 12;
pub const MH_HAS_PRIMITIVE_SHIFT: u32 = 24;
pub const MH_HANDLER_SLOT_BASE_SHIFT: u32 = 25;
pub const MH_HANDLER_SLOT_BASE_BITS: u32 = 8;
pub const MH_FLAGS_SHIFT: u32 = 33;
pub const MH_FLAG_IS_HANDLER: u64 = 1;
pub const MH_FLAG_IS_ENSURE: u64 = 2;

// --- CompiledBlock blockInfo packing (§9) ---
pub const BI_NUM_CAPTURED_SHIFT: u32 = 0;
pub const BI_NUM_CAPTURED_BITS: u32 = 8;
pub const BI_HAS_NLR_SHIFT: u32 = 8;

// --- BlockClosure slots (§10) ---
pub const CLOSURE_COMPILED_BLOCK: usize = 0;
pub const CLOSURE_HOME_PROCESS: usize = 1;
pub const CLOSURE_HOME_OFFSET: usize = 2;
pub const CLOSURE_HOME_SERIAL: usize = 3;
pub const CLOSURE_CAPTURED_BASE: usize = 4;

// --- Send-site entry layout (§8) ---
pub const SITE_SELECTOR: usize = 0;
pub const SITE_ARGC: usize = 1;
pub const SITE_CACHE_CLASS: usize = 2;
pub const SITE_CACHE_METHOD: usize = 3;
pub const SITE_STATIC_CLASS: usize = 4;
pub const SITE_STRIDE: usize = 5;

// --- Special objects array (A.4) ---
pub const SPECIAL_NIL: usize = 0;
pub const SPECIAL_TRUE: usize = 1;
pub const SPECIAL_FALSE: usize = 2;
pub const SPECIAL_SMALLTALK: usize = 3;
pub const SPECIAL_PROCESSOR: usize = 4;
pub const SPECIAL_CLASS_LIST: usize = 5;
pub const SPECIAL_SYMBOL_TABLE: usize = 6;
pub const SPECIAL_SPECIALIZED_SELECTORS: usize = 7;
pub const SPECIAL_SEL_DOES_NOT_UNDERSTAND: usize = 8;
pub const SPECIAL_SEL_MUST_BE_BOOLEAN: usize = 9;
pub const SPECIAL_TERMINATE_TRAMPOLINE: usize = 10;
pub const SPECIAL_LOW_SPACE_SEMAPHORE: usize = 11;
pub const SPECIAL_TIMER_SEMAPHORE: usize = 12;
pub const SPECIAL_OBJECTS_COUNT: usize = 13;

// --- Primitive numbers (A.3) ---
pub const PRIM_CLASS: u16 = 1;
pub const PRIM_IDENTITY_HASH: u16 = 2;
pub const PRIM_IDENTICAL: u16 = 3;
pub const PRIM_NEW: u16 = 4;
pub const PRIM_NEW_SIZED: u16 = 5;
pub const PRIM_AT: u16 = 6;
pub const PRIM_AT_PUT: u16 = 7;
pub const PRIM_SIZE: u16 = 8;
pub const PRIM_INST_VAR_AT: u16 = 9;
pub const PRIM_INST_VAR_AT_PUT: u16 = 10;
pub const PRIM_PERFORM_WITH_ARGS: u16 = 11;
pub const PRIM_INT_ADD: u16 = 100;
pub const PRIM_INT_SUB: u16 = 101;
pub const PRIM_INT_MUL: u16 = 102;
pub const PRIM_INT_DIV: u16 = 103;
pub const PRIM_INT_MOD: u16 = 104;
pub const PRIM_INT_QUO: u16 = 105;
pub const PRIM_INT_LT: u16 = 106;
pub const PRIM_INT_GT: u16 = 107;
pub const PRIM_INT_LE: u16 = 108;
pub const PRIM_INT_GE: u16 = 109;
pub const PRIM_INT_EQ: u16 = 110;
pub const PRIM_INT_BIT_AND: u16 = 111;
pub const PRIM_INT_BIT_OR: u16 = 112;
pub const PRIM_INT_BIT_XOR: u16 = 113;
pub const PRIM_INT_BIT_SHIFT: u16 = 114;
pub const PRIM_INT_AS_FLOAT: u16 = 115;
pub const PRIM_FLOAT_ADD: u16 = 130;
pub const PRIM_FLOAT_SUB: u16 = 131;
pub const PRIM_FLOAT_MUL: u16 = 132;
pub const PRIM_FLOAT_DIV: u16 = 133;
pub const PRIM_FLOAT_LT: u16 = 134;
pub const PRIM_FLOAT_GT: u16 = 135;
pub const PRIM_FLOAT_LE: u16 = 136;
pub const PRIM_FLOAT_GE: u16 = 137;
pub const PRIM_FLOAT_EQ: u16 = 138;
pub const PRIM_FLOAT_TRUNCATED: u16 = 139;
pub const PRIM_FLOAT_SQRT: u16 = 140;
pub const PRIM_BLOCK_VALUE_0: u16 = 200;
pub const PRIM_BLOCK_VALUE_1: u16 = 201;
pub const PRIM_BLOCK_VALUE_2: u16 = 202;
pub const PRIM_BLOCK_VALUE_3: u16 = 203;
pub const PRIM_BLOCK_VALUE_4: u16 = 204;
pub const PRIM_BLOCK_VALUE_ARGS: u16 = 205;
pub const PRIM_TRANSFER_TO: u16 = 210;
pub const PRIM_SEMAPHORE_WAIT: u16 = 211;
pub const PRIM_SEMAPHORE_SIGNAL: u16 = 212;
pub const PRIM_YIELD: u16 = 213;
pub const PRIM_PROCESS_SUSPEND: u16 = 214;
pub const PRIM_PROCESS_RESUME: u16 = 215;
pub const PRIM_PROCESS_TERMINATE: u16 = 216;
pub const PRIM_FIND_HANDLER: u16 = 220;
pub const PRIM_UNWIND_TO: u16 = 221;
/// v1 helper prims for the in-image exception system (§11): read a handler
/// frame's reserved slots, set its state, and capture the signal frame.
pub const PRIM_HANDLER_INFO: u16 = 222;
pub const PRIM_SET_HANDLER_STATE: u16 = 223;
pub const PRIM_SIGNAL_CONTEXT: u16 = 224;
pub const PRIM_FILE_OPEN: u16 = 300;
pub const PRIM_FILE_CLOSE: u16 = 301;
pub const PRIM_FILE_READ: u16 = 302;
pub const PRIM_FILE_WRITE: u16 = 303;
pub const PRIM_FILE_POSITION: u16 = 304;
pub const PRIM_FILE_SET_POSITION: u16 = 305;
pub const PRIM_FILE_SIZE: u16 = 306;
pub const PRIM_FILE_DELETE: u16 = 307;
pub const PRIM_STDIO_WRITE: u16 = 310;
pub const PRIM_STDIO_READ: u16 = 311;
pub const PRIM_CLOCK_MONOTONIC_MS: u16 = 320;
pub const PRIM_CLOCK_WALL_MS: u16 = 321;
pub const PRIM_SIGNAL_AT_MS: u16 = 322;
/// High-resolution monotonic clock for per-frame UI profiling (UI.md §4.4).
/// Honors the virtual clock in headless/deterministic mode.
pub const PRIM_CLOCK_MONOTONIC_NS: u16 = 323;
pub const PRIM_NEXT_EVENT: u16 = 330;
pub const PRIM_PIXEL_BLIT: u16 = 331;
/// The BitBlt workhorse — the one drawing op in Rust (UI.md §4.3).
pub const PRIM_BITBLT: u16 = 332;
/// Save a Form to a PNG file — screenshots for operability (UI.md §4.4).
pub const PRIM_SAVE_FORM: u16 = 333;
/// Push a scripted event onto the host queue — lets in-image tests drive
/// the event pipeline exactly as a real device would (UI.md §4.2, §12).
pub const PRIM_POST_EVENT: u16 = 334;
pub const PRIM_SNAPSHOT: u16 = 400;
pub const PRIM_REGISTER_CLASS: u16 = 401;
pub const PRIM_METHOD_INSTALL: u16 = 402;
pub const PRIM_FLUSH_CACHES: u16 = 403;
pub const PRIM_FRAME_INFO: u16 = 410;
/// Profiling band 420–439 (docs/profiling-plan.md §4).
pub const PRIM_PROFILER_START: u16 = 420;
pub const PRIM_PROFILER_STOP: u16 = 421;
pub const PRIM_PROFILER_REPORT: u16 = 422;
/// Reserved for the tree report (v1.5); unimplemented, fails cleanly.
pub const PRIM_PROFILER_TREE: u16 = 423;
pub const PRIM_VM_COUNTERS: u16 = 424;
pub const PRIM_VM_COUNTERS_RESET: u16 = 425;
pub const PRIM_PROFILER_GATE: u16 = 426;
pub const PRIM_TABLE_SIZE: usize = 4096;

// --- Image header (A.5) ---
pub const IMG_MAGIC_OFFSET: usize = 0;
pub const IMG_VERSION_OFFSET: usize = 4;
pub const IMG_FLAGS_OFFSET: usize = 8;
pub const IMG_SAVED_BASE_OFFSET: usize = 16;
pub const IMG_OLD_SPACE_SIZE_OFFSET: usize = 24;
pub const IMG_SPECIAL_OBJECTS_OFFSET: usize = 32;
pub const IMG_CLASS_LIST_OFFSET: usize = 40;
pub const IMG_ACTIVE_PROCESS_OFFSET: usize = 48;
pub const IMG_RESERVED_OFFSET: usize = 56;
pub const IMG_HEADER_SIZE: usize = 64;
pub const IMG_MAGIC: u32 = 0x4D495453; // "STIM" read little-endian

// --- Limits (§6, §7, §14) ---
pub const MAX_FRAME_SLOTS: usize = 255;
pub const MAX_METHOD_INSTRUCTIONS: usize = 65536;
pub const MAX_LITERALS: usize = 65536;
pub const MAX_SEND_SITES: usize = 255;
pub const LOOKUP_CACHE_SIZE: usize = 4096;
pub const INITIAL_STACK_BYTES: usize = 4096;
pub const DEFAULT_MAX_STACK_BYTES: usize = 16 * 1024 * 1024;
pub const YOUNG_SPACE_BYTES_DEFAULT: usize = 8 * 1024 * 1024;
pub const LARGE_OBJECT_BYTES: usize = 65536;
pub const TENURE_AGE: u64 = 4;

/// The Smalltalk mirror of the Treaty (SPEC §20 Phase 0): one class-side
/// method per constant, name camelCased from the canonical name. Generated
/// by `cargo run --bin gen_treaty_st`; the `treaty_st_is_current` test
/// asserts the on-disk copy matches.
pub fn treaty_st_source() -> String {
    fn camel(name: &str) -> String {
        let mut out = String::new();
        for (i, part) in name.split('_').enumerate() {
            let lower = part.to_lowercase();
            if i == 0 {
                out.push_str(&lower);
            } else {
                let mut cs = lower.chars();
                if let Some(c) = cs.next() {
                    out.push(c.to_ascii_uppercase());
                    out.push_str(cs.as_str());
                }
            }
        }
        out
    }
    let mut s = String::new();
    s.push_str(
        "\"GENERATED from treaty.json by `cargo run --bin gen_treaty_st` — DO NOT EDIT.\n\
         The Treaty (SPEC Appendix A): the binary contract between the VM and the\n\
         compiler. One class-side method per constant.\"\n\n\
         Object subclass: #Treaty\n\
         \tinstanceVariableNames: ''\n\
         \tclassVariableNames: ''\n\
         \tpoolDictionaries: ''\n\
         \tcategory: 'Smallishtalk-Compiler'!\n\n\
         !Treaty class methodsFor: 'constants'!\n",
    );
    for (group, name, value) in all_constants() {
        // Opcode names are bare mnemonics in the JSON; prefix them so the
        // selectors are unmistakable (and can't shadow real protocol).
        let full = if group == "opcodes" {
            format!("OP_{name}")
        } else {
            name.to_string()
        };
        s.push_str(&format!(
            "{}\n\t\"{}::{}\"\n\t^{}!\n",
            camel(&full),
            group,
            name,
            value
        ));
    }
    s.push_str(" !\n");
    s
}

/// Every Treaty constant as (json_group, json_key, value), for the
/// agreement test against treaty.json.
pub fn all_constants() -> Vec<(&'static str, &'static str, u64)> {
    macro_rules! c {
        ($($group:literal : $($name:ident),+ ;)+) => {
            vec![ $( $( ($group, stringify!($name), $name as u64), )+ )+ ]
        };
    }
    let mut v = c! {
        "tags": TAG_INT_BIT, TAG_PTR_MASK, TAG_PTR, TAG_FLOAT_IMM, SMALLINT_BITS;
        "header": HDR_CLASS_SHIFT, HDR_CLASS_BITS, HDR_HASH_SHIFT, HDR_HASH_BITS,
            HDR_NSLOTS_SHIFT, HDR_NSLOTS_BITS, HDR_NSLOTS_OVERFLOW,
            HDR_FORMAT_SHIFT, HDR_FORMAT_BITS, HDR_GC_SHIFT, HDR_GC_BITS,
            GC_BIT_MARK, GC_BIT_REMEMBERED, GC_BIT_PINNED, GC_AGE_SHIFT,
            GC_AGE_BITS, GC_BIT_IMMUTABLE;
        "formats": FMT_FIXED, FMT_PTRS, FMT_BYTES_BASE;
        "frame": FRAME_CALLER, FRAME_RETINFO, FRAME_METHOD, FRAME_FLAGS,
            FRAME_RECEIVER, FRAME_FIXED, FLAG_HANDLER, FLAG_ENSURE, FLAG_BLOCKCTX,
            FLAG_UNWINDCONT, SERIAL_SHIFT, RETINFO_DEST_BITS, RETINFO_PC_SHIFT;
        "stack_slots": STACK_OWNER, STACK_FRAMES_BASE;
        "method_dictionary_slots": MDICT_KEYS, MDICT_VALUES, MDICT_NUM_VM_SLOTS;
        "linked_list_slots": LIST_HEAD, LIST_TAIL, LIST_NUM_VM_SLOTS;
        "handler_slots": HANDLER_SLOT_CLASS, HANDLER_SLOT_BLOCK, HANDLER_SLOT_STATE,
            HANDLER_STATE_ARMED, HANDLER_STATE_IN_PROGRESS, ENSURE_SLOT_BLOCK,
            ENSURE_SLOT_PENDING_TARGET, ENSURE_SLOT_PENDING_SERIAL,
            ENSURE_SLOT_PENDING_VALUE;
        "specialized_selectors": SPECSEL_PLUS, SPECSEL_MINUS, SPECSEL_TIMES,
            SPECSEL_INT_DIV, SPECSEL_MOD, SPECSEL_LT, SPECSEL_GT, SPECSEL_LE,
            SPECSEL_GE, SPECSEL_EQ, SPECSEL_IDENTICAL, SPECSEL_AT, SPECSEL_AT_PUT,
            SPECSEL_SIZE, SPECSEL_CLASS, SPECSEL_NOT, SPECSEL_COUNT;
        "classes": CLASS_OBJECT, CLASS_BEHAVIOR, CLASS_CLASS, CLASS_METACLASS,
            CLASS_UNDEFINED_OBJECT, CLASS_TRUE, CLASS_FALSE, CLASS_SMALLINTEGER,
            CLASS_FLOAT, CLASS_CHARACTER, CLASS_STRING, CLASS_BYTESTRING,
            CLASS_SYMBOL, CLASS_LARGE_POSITIVE_INTEGER, CLASS_LARGE_NEGATIVE_INTEGER,
            CLASS_ARRAY, CLASS_BYTEARRAY, CLASS_ORDERED_COLLECTION, CLASS_ASSOCIATION,
            CLASS_BOX, CLASS_BLOCKCLOSURE, CLASS_COMPILEDMETHOD, CLASS_COMPILEDBLOCK,
            CLASS_PROCESS, CLASS_SEMAPHORE, CLASS_METHODDICTIONARY,
            CLASS_PROCESSOR_SCHEDULER, CLASS_MESSAGE, CLASS_SYSTEM_DICTIONARY,
            CLASS_STACK, CLASS_LINKED_LIST, FIRST_UNRESERVED_CLASS_INDEX;
        "behavior_slots": BEHAVIOR_SUPERCLASS, BEHAVIOR_METHOD_DICTIONARY,
            BEHAVIOR_FORMAT_AND_SLOTS, BEHAVIOR_CLASS_INDEX, BEHAVIOR_NUM_VM_SLOTS,
            FORMAT_AND_SLOTS_FORMAT_SHIFT, FORMAT_AND_SLOTS_NSLOTS_MASK;
        "process_slots": PROCESS_STACK, PROCESS_FRAME_OFFSET, PROCESS_PC,
            PROCESS_PRIORITY, PROCESS_NEXT_LINK, PROCESS_MY_LIST,
            PROCESS_SERIAL_COUNTER, PROCESS_NUM_VM_SLOTS;
        "semaphore_slots": SEMAPHORE_EXCESS_SIGNALS, SEMAPHORE_QUEUE_HEAD,
            SEMAPHORE_QUEUE_TAIL, SEMAPHORE_NUM_VM_SLOTS;
        "scheduler_slots": SCHEDULER_QUEUES, SCHEDULER_ACTIVE_PROCESS,
            SCHEDULER_NUM_VM_SLOTS, NUM_PRIORITIES;
        "compiled_method_slots": METHOD_HEADER, METHOD_BYTECODES, METHOD_LITERALS,
            METHOD_SEND_SITES, METHOD_SELECTOR, METHOD_CLASS, METHOD_SOURCE_INFO,
            METHOD_NUM_SLOTS;
        "compiled_block_slots": BLOCK_HEADER, BLOCK_BYTECODES, BLOCK_LITERALS,
            BLOCK_SEND_SITES, BLOCK_OUTER_METHOD, BLOCK_INFO, BLOCK_NUM_SLOTS;
        "method_header": MH_FRAME_SLOTS_SHIFT, MH_FRAME_SLOTS_BITS, MH_ARGC_SHIFT,
            MH_ARGC_BITS, MH_PRIMITIVE_SHIFT, MH_PRIMITIVE_BITS,
            MH_HAS_PRIMITIVE_SHIFT, MH_HANDLER_SLOT_BASE_SHIFT,
            MH_HANDLER_SLOT_BASE_BITS, MH_FLAGS_SHIFT, MH_FLAG_IS_HANDLER,
            MH_FLAG_IS_ENSURE;
        "block_info": BI_NUM_CAPTURED_SHIFT, BI_NUM_CAPTURED_BITS, BI_HAS_NLR_SHIFT;
        "closure_slots": CLOSURE_COMPILED_BLOCK, CLOSURE_HOME_PROCESS,
            CLOSURE_HOME_OFFSET, CLOSURE_HOME_SERIAL, CLOSURE_CAPTURED_BASE;
        "send_site": SITE_SELECTOR, SITE_ARGC, SITE_CACHE_CLASS, SITE_CACHE_METHOD,
            SITE_STATIC_CLASS, SITE_STRIDE;
        "special_objects": SPECIAL_NIL, SPECIAL_TRUE, SPECIAL_FALSE,
            SPECIAL_SMALLTALK, SPECIAL_PROCESSOR, SPECIAL_CLASS_LIST,
            SPECIAL_SYMBOL_TABLE, SPECIAL_SPECIALIZED_SELECTORS,
            SPECIAL_SEL_DOES_NOT_UNDERSTAND, SPECIAL_SEL_MUST_BE_BOOLEAN,
            SPECIAL_TERMINATE_TRAMPOLINE, SPECIAL_LOW_SPACE_SEMAPHORE,
            SPECIAL_TIMER_SEMAPHORE, SPECIAL_OBJECTS_COUNT;
        "primitives": PRIM_CLASS, PRIM_IDENTITY_HASH, PRIM_IDENTICAL, PRIM_NEW,
            PRIM_NEW_SIZED, PRIM_AT, PRIM_AT_PUT, PRIM_SIZE, PRIM_INST_VAR_AT,
            PRIM_INST_VAR_AT_PUT, PRIM_PERFORM_WITH_ARGS,
            PRIM_INT_ADD, PRIM_INT_SUB, PRIM_INT_MUL, PRIM_INT_DIV, PRIM_INT_MOD,
            PRIM_INT_QUO, PRIM_INT_LT, PRIM_INT_GT, PRIM_INT_LE, PRIM_INT_GE,
            PRIM_INT_EQ, PRIM_INT_BIT_AND, PRIM_INT_BIT_OR, PRIM_INT_BIT_XOR,
            PRIM_INT_BIT_SHIFT, PRIM_INT_AS_FLOAT,
            PRIM_FLOAT_ADD, PRIM_FLOAT_SUB, PRIM_FLOAT_MUL, PRIM_FLOAT_DIV,
            PRIM_FLOAT_LT, PRIM_FLOAT_GT, PRIM_FLOAT_LE, PRIM_FLOAT_GE,
            PRIM_FLOAT_EQ, PRIM_FLOAT_TRUNCATED, PRIM_FLOAT_SQRT,
            PRIM_BLOCK_VALUE_0, PRIM_BLOCK_VALUE_1, PRIM_BLOCK_VALUE_2,
            PRIM_BLOCK_VALUE_3, PRIM_BLOCK_VALUE_4, PRIM_BLOCK_VALUE_ARGS,
            PRIM_TRANSFER_TO, PRIM_SEMAPHORE_WAIT, PRIM_SEMAPHORE_SIGNAL,
            PRIM_YIELD, PRIM_PROCESS_SUSPEND, PRIM_PROCESS_RESUME,
            PRIM_PROCESS_TERMINATE, PRIM_FIND_HANDLER, PRIM_UNWIND_TO,
            PRIM_HANDLER_INFO, PRIM_SET_HANDLER_STATE, PRIM_SIGNAL_CONTEXT,
            PRIM_FILE_OPEN, PRIM_FILE_CLOSE, PRIM_FILE_READ, PRIM_FILE_WRITE,
            PRIM_FILE_POSITION, PRIM_FILE_SET_POSITION, PRIM_FILE_SIZE,
            PRIM_FILE_DELETE, PRIM_STDIO_WRITE, PRIM_STDIO_READ,
            PRIM_CLOCK_MONOTONIC_MS, PRIM_CLOCK_WALL_MS, PRIM_SIGNAL_AT_MS,
            PRIM_CLOCK_MONOTONIC_NS,
            PRIM_NEXT_EVENT, PRIM_PIXEL_BLIT, PRIM_BITBLT, PRIM_SAVE_FORM,
            PRIM_POST_EVENT,
            PRIM_SNAPSHOT, PRIM_REGISTER_CLASS,
            PRIM_METHOD_INSTALL, PRIM_FLUSH_CACHES, PRIM_FRAME_INFO,
            PRIM_PROFILER_START, PRIM_PROFILER_STOP, PRIM_PROFILER_REPORT,
            PRIM_PROFILER_TREE, PRIM_VM_COUNTERS, PRIM_VM_COUNTERS_RESET,
            PRIM_PROFILER_GATE, PRIM_TABLE_SIZE;
        "image_header": IMG_MAGIC_OFFSET, IMG_VERSION_OFFSET, IMG_FLAGS_OFFSET,
            IMG_SAVED_BASE_OFFSET, IMG_OLD_SPACE_SIZE_OFFSET,
            IMG_SPECIAL_OBJECTS_OFFSET, IMG_CLASS_LIST_OFFSET,
            IMG_ACTIVE_PROCESS_OFFSET, IMG_RESERVED_OFFSET, IMG_HEADER_SIZE,
            IMG_MAGIC;
        "limits": MAX_FRAME_SLOTS, MAX_METHOD_INSTRUCTIONS, MAX_LITERALS,
            MAX_SEND_SITES, LOOKUP_CACHE_SIZE, INITIAL_STACK_BYTES,
            DEFAULT_MAX_STACK_BYTES, YOUNG_SPACE_BYTES_DEFAULT,
            LARGE_OBJECT_BYTES, TENURE_AGE;
    };
    // Rust names carry an OP_ prefix; the JSON uses bare mnemonics.
    let ops: &[(&str, u8)] = &[
        ("NOP", OP_NOP), ("BREAK", OP_BREAK), ("MOVE", OP_MOVE),
        ("LOADK", OP_LOADK), ("LOADINT", OP_LOADINT), ("LOADNIL", OP_LOADNIL),
        ("LOADTRUE", OP_LOADTRUE), ("LOADFALSE", OP_LOADFALSE),
        ("LOADSELF", OP_LOADSELF), ("GETIVAR", OP_GETIVAR), ("SETIVAR", OP_SETIVAR),
        ("GETBOX", OP_GETBOX), ("SETBOX", OP_SETBOX), ("MKBOX", OP_MKBOX),
        ("SEND", OP_SEND), ("SENDSUPER", OP_SENDSUPER), ("RET", OP_RET),
        ("RETSELF", OP_RETSELF), ("NLR", OP_NLR), ("PRIM", OP_PRIM),
        ("MKCLOSURE", OP_MKCLOSURE), ("CAPTURE", OP_CAPTURE), ("JUMP", OP_JUMP),
        ("JUMPTRUE", OP_JUMPTRUE), ("JUMPFALSE", OP_JUMPFALSE),
        ("ADD", OP_ADD), ("SUB", OP_SUB), ("MUL", OP_MUL), ("DIV", OP_DIV),
        ("MOD", OP_MOD), ("LT", OP_LT), ("GT", OP_GT), ("LE", OP_LE),
        ("GE", OP_GE), ("EQNUM", OP_EQNUM), ("AT", OP_AT), ("ATPUT", OP_ATPUT),
        ("SIZE", OP_SIZE), ("CLASSOF", OP_CLASSOF), ("NOT", OP_NOT),
        ("IDEQ", OP_IDEQ),
    ];
    for (name, op) in ops {
        v.push(("opcodes", name, *op as u64));
    }
    v
}
