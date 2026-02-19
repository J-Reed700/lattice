/**
 * DropZone Usage Examples
 *
 * This file demonstrates various ways to use the DropZone component
 */

import { useState } from 'react';

import { type Accept } from 'react-dropzone';

import { DropZone } from './DropZone';

/**
 * Example 1: Basic usage with all file types
 */
export function BasicDropZoneExample() {
  const handleDrop = (files: File[]) => {
    console.log('Dropped files:', files);
    files.forEach(file => {
      console.log(`- ${file.name} (${file.type}, ${file.size} bytes)`);
    });
  };

  return (
    <div className="p-6">
      <h2 className="text-2xl font-bold mb-4">Basic DropZone</h2>
      <DropZone onDrop={handleDrop} />
    </div>
  );
}

/**
 * Example 2: Document-only upload
 */
export function DocumentOnlyDropZone() {
  const [uploadedFiles, setUploadedFiles] = useState<File[]>([]);

  const documentTypes: Accept = {
    'application/pdf': ['.pdf'],
    'application/msword': ['.doc'],
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document': ['.docx'],
    'text/plain': ['.txt'],
    'text/markdown': ['.md'],
  };

  const handleDrop = (files: File[]) => {
    setUploadedFiles(prev => [...prev, ...files]);
  };

  return (
    <div className="p-6">
      <h2 className="text-2xl font-bold mb-4">Document Upload</h2>
      <DropZone
        onDrop={handleDrop}
        accept={documentTypes}
        maxSize={50 * 1024 * 1024}
        multiple
      />

      {uploadedFiles.length > 0 && (
        <div className="mt-4">
          <h3 className="font-semibold mb-2">Uploaded Files:</h3>
          <ul className="list-disc list-inside">
            {uploadedFiles.map((file, index) => (
              <li key={index}>{file.name}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/**
 * Example 3: Image upload with preview
 */
export function ImageDropZone() {
  const [imagePreviews, setImagePreviews] = useState<string[]>([]);

  const imageTypes: Accept = {
    'image/jpeg': ['.jpg', '.jpeg'],
    'image/png': ['.png'],
    'image/gif': ['.gif'],
    'image/webp': ['.webp'],
  };

  const handleDrop = (files: File[]) => {
    files.forEach(file => {
      const reader = new FileReader();
      reader.onload = (e) => {
        if (e.target?.result) {
          setImagePreviews(prev => [...prev, e.target!.result as string]);
        }
      };
      reader.readAsDataURL(file);
    });
  };

  return (
    <div className="p-6">
      <h2 className="text-2xl font-bold mb-4">Image Upload with Preview</h2>
      <DropZone
        onDrop={handleDrop}
        accept={imageTypes}
        maxSize={10 * 1024 * 1024}
      />

      {imagePreviews.length > 0 && (
        <div className="mt-4 grid grid-cols-3 gap-4">
          {imagePreviews.map((preview, index) => (
            <img
              key={index}
              src={preview}
              alt={`Preview ${index}`}
              className="w-full h-32 object-cover rounded-lg"
            />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Example 4: Single file upload
 */
export function SingleFileDropZone() {
  const [file, setFile] = useState<File | null>(null);

  const handleDrop = (files: File[]) => {
    if (files.length > 0) {
      setFile(files[0]);
    }
  };

  return (
    <div className="p-6">
      <h2 className="text-2xl font-bold mb-4">Single File Upload</h2>
      <DropZone
        onDrop={handleDrop}
        multiple={false}
        maxSize={25 * 1024 * 1024}
      />

      {file && (
        <div className="mt-4 p-4 bg-[var(--bg-secondary)] rounded-lg">
          <h3 className="font-semibold">Selected File:</h3>
          <p className="text-[var(--text-secondary)]">{file.name}</p>
          <p className="text-sm text-[var(--text-tertiary)]">
            {(file.size / 1024).toFixed(2)} KB
          </p>
        </div>
      )}
    </div>
  );
}

/**
 * Example 5: With loading state
 */
export function DropZoneWithUpload() {
  const [isUploading, setIsUploading] = useState(false);
  const [uploadedCount, setUploadedCount] = useState(0);

  const handleDrop = async (files: File[]) => {
    setIsUploading(true);

    try {
      for (const file of files) {
        await simulateUpload(file);
        setUploadedCount(prev => prev + 1);
      }

      alert(`Successfully uploaded ${files.length} file(s)!`);
    } catch (error) {
      console.error('Upload failed:', error);
      alert('Upload failed. Please try again.');
    } finally {
      setIsUploading(false);
    }
  };

  const simulateUpload = (file: File): Promise<void> => new Promise(resolve => {
      setTimeout(() => {
        console.log(`Uploaded: ${file.name}`);
        resolve();
      }, 1000);
    });

  return (
    <div className="p-6">
      <h2 className="text-2xl font-bold mb-4">Upload with Progress</h2>
      <DropZone
        onDrop={handleDrop}
        disabled={isUploading}
      />

      {isUploading && (
        <div className="mt-4 text-center">
          <p className="text-[var(--accent-primary)] font-semibold">
            Uploading... ({uploadedCount} files processed)
          </p>
        </div>
      )}
    </div>
  );
}

/**
 * Example 6: Custom styling
 */
export function CustomStyledDropZone() {
  const handleDrop = (files: File[]) => {
    console.log('Files dropped:', files);
  };

  return (
    <div className="p-6">
      <h2 className="text-2xl font-bold mb-4">Custom Styled DropZone</h2>
      <DropZone
        onDrop={handleDrop}
        className="min-h-[400px] gradient-brand-subtle"
      />
    </div>
  );
}
