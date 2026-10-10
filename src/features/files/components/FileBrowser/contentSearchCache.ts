/** Bounds retained file text and rejects stale entries after a document changes. */
export class ContentSearchCache {
  private entries = new Map<string, { version: string; text: string }>();
  private size = 0;

  constructor(private readonly maxCharacters = 4_000_000) {}

  get(id: string, version: string): string | undefined {
    const entry = this.entries.get(id);
    if (!entry) return undefined;
    if (entry.version !== version) {
      this.delete(id);
      return undefined;
    }
    this.entries.delete(id);
    this.entries.set(id, entry);
    return entry.text;
  }

  set(id: string, version: string, text: string): void {
    this.delete(id);
    if (text.length > this.maxCharacters) return;
    while (this.size + text.length > this.maxCharacters && this.entries.size) {
      this.delete(this.entries.keys().next().value!);
    }
    this.entries.set(id, { version, text });
    this.size += text.length;
  }

  retain(ids: Set<string>): void {
    for (const id of this.entries.keys()) if (!ids.has(id)) this.delete(id);
  }

  private delete(id: string): void {
    const entry = this.entries.get(id);
    if (entry) this.size -= entry.text.length;
    this.entries.delete(id);
  }
}
