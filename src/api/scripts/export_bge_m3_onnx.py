#!/usr/bin/env python3
"""
Export BGE-M3 model to ONNX format for use in the Desktop (Rust) application.

This script exports the BAAI/bge-m3 model to ONNX format with optional quantization
for smaller file size and faster inference on CPU.

Usage:
    python export_bge_m3_onnx.py [--quantize] [--output-dir OUTPUT_DIR]

Requirements:
    pip install optimum[exporters] onnx onnxruntime sentence-transformers
"""

import argparse
import logging
from pathlib import Path
from typing import Optional
import sys

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

def export_bge_m3_to_onnx(
    output_dir: Path,
    quantize: bool = False,
    optimize: bool = True
) -> None:
    """
    Export BGE-M3 model to ONNX format.

    Args:
        output_dir: Directory to save the exported model
        quantize: Whether to apply dynamic quantization (reduces size, faster CPU inference)
        optimize: Whether to optimize the ONNX model
    """
    try:
        from optimum.onnxruntime import ORTModelForFeatureExtraction
        from transformers import AutoTokenizer
        import onnx
        from onnxruntime.quantization import quantize_dynamic, QuantType

        MODEL_NAME = "BAAI/bge-m3"

        logger.info(f"Exporting {MODEL_NAME} to ONNX format...")
        logger.info(f"Output directory: {output_dir}")

        # Create output directory
        output_dir.mkdir(parents=True, exist_ok=True)

        # Export model to ONNX
        logger.info("Loading and converting model to ONNX...")
        model = ORTModelForFeatureExtraction.from_pretrained(
            MODEL_NAME,
            export=True,
            provider="CPUExecutionProvider"
        )

        # Save the model
        model_path = output_dir / "model.onnx"
        model.save_pretrained(str(output_dir))
        logger.info(f"Model saved to {output_dir}")

        # Export tokenizer
        logger.info("Exporting tokenizer...")
        tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME)
        tokenizer.save_pretrained(str(output_dir))
        logger.info(f"Tokenizer saved to {output_dir}")

        # Optimize ONNX model
        if optimize:
            logger.info("Optimizing ONNX model...")
            try:
                from onnxruntime.transformers import optimizer
                from onnxruntime.transformers.onnx_model_bert import BertOnnxModel

                optimized_model_path = output_dir / "model_optimized.onnx"

                # Load and optimize
                model_to_optimize = onnx.load(str(model_path))

                # Basic optimization
                opt_model = optimizer.optimize_model(
                    str(model_path),
                    model_type='bert',
                    num_heads=12,
                    hidden_size=1024
                )
                opt_model.save_model_to_file(str(optimized_model_path))

                logger.info(f"Optimized model saved to {optimized_model_path}")

                # Replace original with optimized
                import shutil
                shutil.move(str(optimized_model_path), str(model_path))
                logger.info("Replaced original model with optimized version")

            except Exception as e:
                logger.warning(f"Optimization failed (non-critical): {e}")

        # Quantize if requested
        if quantize:
            logger.info("Applying dynamic quantization (INT8)...")
            quantized_model_path = output_dir / "model_quantized.onnx"

            quantize_dynamic(
                str(model_path),
                str(quantized_model_path),
                weight_type=QuantType.QInt8
            )

            logger.info(f"Quantized model saved to {quantized_model_path}")

            # Show size comparison
            original_size = model_path.stat().st_size / (1024 * 1024)
            quantized_size = quantized_model_path.stat().st_size / (1024 * 1024)

            logger.info(f"Original model size: {original_size:.2f} MB")
            logger.info(f"Quantized model size: {quantized_size:.2f} MB")
            logger.info(f"Size reduction: {(1 - quantized_size/original_size) * 100:.1f}%")

        # Verify the exported model
        logger.info("Verifying exported model...")
        verify_onnx_model(output_dir, quantize)

        logger.info("✓ Export completed successfully!")
        logger.info(f"\nFiles created in {output_dir}:")
        for file in output_dir.iterdir():
            if file.is_file():
                size_mb = file.stat().st_size / (1024 * 1024)
                logger.info(f"  - {file.name} ({size_mb:.2f} MB)")

    except ImportError as e:
        logger.error(f"Missing required dependency: {e}")
        logger.error("Install with: pip install optimum[exporters] onnx onnxruntime sentence-transformers")
        sys.exit(1)
    except Exception as e:
        logger.error(f"Export failed: {e}")
        raise


def verify_onnx_model(output_dir: Path, use_quantized: bool = False) -> None:
    """
    Verify the exported ONNX model produces correct embeddings.

    Args:
        output_dir: Directory containing the exported model
        use_quantized: Whether to test the quantized version
    """
    import onnxruntime as ort
    import numpy as np
    from transformers import AutoTokenizer

    model_file = "model_quantized.onnx" if use_quantized and (output_dir / "model_quantized.onnx").exists() else "model.onnx"
    model_path = output_dir / model_file

    logger.info(f"Loading ONNX model from {model_path}...")

    # Create inference session
    session = ort.InferenceSession(str(model_path), providers=['CPUExecutionProvider'])

    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(str(output_dir))

    # Test with sample text
    test_text = "This is a test sentence for BGE-M3 embeddings."

    logger.info(f"Testing with: '{test_text}'")

    # Tokenize
    inputs = tokenizer(test_text, return_tensors="np", padding=True, truncation=True, max_length=512)

    # Prepare inputs for ONNX
    onnx_inputs = {
        "input_ids": inputs["input_ids"].astype(np.int64),
        "attention_mask": inputs["attention_mask"].astype(np.int64),
    }

    # Run inference
    outputs = session.run(None, onnx_inputs)

    # Get embeddings (typically last hidden state or pooler output)
    embeddings = outputs[0]

    # Mean pooling
    attention_mask = onnx_inputs["attention_mask"]
    embeddings_pooled = np.sum(embeddings * attention_mask[:, :, np.newaxis], axis=1) / np.sum(attention_mask, axis=1, keepdims=True)

    # Normalize
    embeddings_normalized = embeddings_pooled / np.linalg.norm(embeddings_pooled, axis=1, keepdims=True)

    logger.info(f"✓ Embedding shape: {embeddings_normalized.shape}")
    logger.info(f"✓ Embedding dimension: {embeddings_normalized.shape[1]}")

    if embeddings_normalized.shape[1] != 1024:
        logger.error(f"ERROR: Expected dimension 1024, got {embeddings_normalized.shape[1]}")
        sys.exit(1)

    logger.info(f"✓ Model verification passed!")


def main():
    parser = argparse.ArgumentParser(
        description="Export BGE-M3 model to ONNX format",
        formatter_class=argparse.RawDescriptionHelpFormatter
    )

    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path(__file__).parent.parent / "models" / "bge-m3-onnx",
        help="Output directory for exported model (default: ../models/bge-m3-onnx)"
    )

    parser.add_argument(
        "--quantize",
        action="store_true",
        help="Apply dynamic quantization (INT8) for smaller size and faster CPU inference"
    )

    parser.add_argument(
        "--no-optimize",
        action="store_true",
        help="Skip ONNX model optimization"
    )

    args = parser.parse_args()

    logger.info("=" * 70)
    logger.info("BGE-M3 ONNX Export Tool")
    logger.info("=" * 70)

    export_bge_m3_to_onnx(
        output_dir=args.output_dir,
        quantize=args.quantize,
        optimize=not args.no_optimize
    )


if __name__ == "__main__":
    main()
