interface ViewerHeaderProps {
  fileName: string;
}

export function ViewerHeader({ fileName }: ViewerHeaderProps) {
  return (
    <div className="flex h-12 shrink-0 items-center border-b border-border-subtle px-4 pr-12">
      <h2 className="m-0 truncate font-serif text-sm font-semibold text-text-primary">
        {fileName}
      </h2>
    </div>
  );
}
