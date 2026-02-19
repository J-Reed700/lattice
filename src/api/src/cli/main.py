"""
Vault Backend CLI - Main entry point.

Provides commands for indexing, searching, and managing the knowledge base.
"""

from pathlib import Path

from rich.console import Console
import typer

console = Console()
app = typer.Typer(
    name="vault",
    help="Personal knowledge base with semantic search and RAG",
    pretty_exceptions_enable=False,  # Disable pretty exceptions
    add_completion=False,
    rich_markup_mode=None,  # Disable rich help formatting
)


@app.command()
def init() -> None:
    """Initialize Recall database and configuration."""
    data_path = Path.home() / ".recall"
    console.print("[bold green]Initializing Recall...[/bold green]")
    console.print(f"Data directory: {data_path}")
    data_path.mkdir(parents=True, exist_ok=True)
    console.print("[green]✓[/green] Recall initialized successfully")


@app.command()
def status() -> None:
    """Show Recall system status."""
    console.print("[bold]Recall System Status[/bold]\n")
    console.print(f"Data Directory: {Path.home() / '.recall'}")
    console.print("Embedding Model: dunzhang/stella_en_1.5B_v5")
    console.print("LLM Model: llama3.1:8b")


@app.command()
def config() -> None:
    """Show Recall configuration."""
    console.print("[bold]Current Configuration:[/bold]\n")
    console.print(f"Data Directory: {Path.home() / '.recall'}")
    console.print("Chunk Size: 512")
    console.print("Embedding Model: dunzhang/stella_en_1.5B_v5")


def main() -> None:
    """Main entry point."""
    app()


if __name__ == "__main__":
    main()
