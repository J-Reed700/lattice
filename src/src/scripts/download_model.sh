#!/bin/bash

MODEL_DIR="$HOME/.lattice/models"
mkdir -p "$MODEL_DIR"

echo "Downloading all-MiniLM-L6-v2 ONNX model..."

cat << 'EOF'
To prepare the ONNX model:

1. Install Python dependencies:
   pip install transformers optimum onnx onnxruntime

2. Convert model to ONNX:
   python -c "
from optimum.onnxruntime import ORTModelForFeatureExtraction
from transformers import AutoTokenizer

model = ORTModelForFeatureExtraction.from_pretrained(
    'sentence-transformers/all-MiniLM-L6-v2',
    export=True
)
tokenizer = AutoTokenizer.from_pretrained('sentence-transformers/all-MiniLM-L6-v2')

import os
model_dir = os.path.expanduser('~/.lattice/models')
model.save_pretrained(model_dir)
tokenizer.save_pretrained(model_dir)

print(f'Model saved to: {model_dir}')
"

3. The model will be saved to: $MODEL_DIR

EOF
