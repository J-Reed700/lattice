interface ViewerHeaderProps {
  fileName: string;
}

export function ViewerHeader({ fileName }: ViewerHeaderProps) {
  return (
    <div className="flex items-center justify-between border-b px-5 py-2.5">
      <div className="flex-1 min-w-0">
        <h2 className="m-0 truncate text-base font-semibold">{fileName}</h2>
      </div>
      {/* Close button removed - using Dialog's default close button */}
    </div>
  );
}
