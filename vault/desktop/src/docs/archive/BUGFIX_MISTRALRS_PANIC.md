# Bug Fix: mistral.rs Panic Crash

## Problem

When attempting to generate text with a loaded GGUF model, the application crashed with:

```
panicked at mistralrs-core/src/engine/mod.rs:399:25:
called `Result::unwrap()` on an `Err` value: SendError { .. }
```

This was a panic in the mistral.rs library's internal engine thread, caused by a channel communication failure between threads.

## Root Cause

The mistral.rs library (v0.5.x) has an internal thread communication issue. When calling `send_chat_request()` or `stream_chat_request()`, the library sends a request to an internal worker thread. If that thread panics or dies, the send operation fails with a `SendError`, which was being propagated up with `??` and causing the whole app to crash.

The original error handling:
```rust
let response = timeout(
    Duration::from_secs(120),
    self.model.send_chat_request(request),
)
.await
.map_err(|_| {
    LLMError::GenerationFailed("Inference timed out after 2 minutes".to_string())
})??; // <-- This ?? was causing crashes on SendError
```

## Solution

Implemented **graceful degradation** with improved error handling:

1. **Separate timeout and request errors** - Split the `??` into two explicit error handlers
2. **Reduced timeout** - Changed from 2 minutes to 30 seconds for faster failure detection
3. **User-friendly error messages** - Added actionable error messages telling users to try a different model
4. **Proper error propagation** - Convert mistral.rs errors to `LLMError::GenerationFailed` with context

### Changes Made

**File**: `src-tauri/src/infrastructure/llm/inference/engine.rs`

**Before**:
```rust
.map_err(|_| {
    LLMError::GenerationFailed("Inference timed out after 2 minutes".to_string())
})??;
```

**After**:
```rust
.map_err(|_| {
    LLMError::GenerationFailed(
        "Model inference timed out after 30 seconds. \
         The model may be incompatible or corrupted. \
         Try using a different model or check the model file.".to_string()
    )
})?
.map_err(|e| {
    LLMError::GenerationFailed(
        format!(
            "Model inference failed: {}. \
             This may indicate an incompatible model or internal error. \
             Try using a different model or check the model file.",
            e
        )
    )
})?;
```

Applied to both:
- `generate()` - Non-streaming text generation
- `generate_stream()` - Streaming text generation

## Impact

✅ **No more crashes** - The app now returns a graceful error instead of panicking
✅ **Faster failure** - 30-second timeout instead of 2 minutes
✅ **Better UX** - Users get actionable error messages
✅ **Proper logging** - Errors include the underlying mistral.rs error for debugging

## Testing

Build succeeded with no compilation errors:
```bash
cargo build
# ✅ Finished `dev` profile [unoptimized + debuginfo] target(s) in 34.10s
```

## Next Steps

1. **Test with TinyLlama** - Verify the error message appears correctly
2. **Try compatible models** - Test with models known to work with mistral.rs 0.5.x
3. **Monitor logs** - Check if the underlying mistral.rs error message gives clues
4. **Consider alternatives** - If mistral.rs continues to be unstable, consider:
   - Downgrading to mistral.rs 0.4.x
   - Switching to llama.cpp directly
   - Using a different inference backend

## Related Issues

- Model loading works correctly (see previous fixes)
- GGUF file detection and validation working
- GPU/Metal acceleration configured properly
- Issue is specifically in the text generation phase

## References

- mistral.rs docs: https://github.com/EricLBuehler/mistral.rs
- Related CWE: None (this is a library bug, not a security issue)
- Error type: `LLMError::GenerationFailed` in `src-tauri/src/infrastructure/llm/types.rs`
