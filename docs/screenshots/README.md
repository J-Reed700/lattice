# Screenshot capture checklist

Images for the root `README.md` live in this folder as PNGs. Capture them
from a working `npm run tauri:dev` session with a small, realistic library
loaded — a handful of notes, a PDF or two, and one conversation with a few
cited answers. Avoid placeholder data and empty states; the README should
show the app doing something.

| File | What to capture |
| --- | --- |
| `dashboard.png` | The Home page: the document/folder/storage tiles, recent documents, and a resumable conversation. |
| `search.png` | A search with results, showing the highlighted match and the file it came from. |
| `chat.png` | A chat answer with its source citations visible, and the retrieval trace open if it fits. |
| `study.png` | A flashcard review session with the card's source citation shown. |
| `neighborhood.png` | A document open with the "Related" panel: backlinks, similar documents, and where it's cited. |
| `settings.png` | Settings with the model catalog or local model roles, showing a downloaded model. |

Conventions:

- Capture at the app's default window size, 2x scale, in one theme (dark is
  fine) so the set reads as one session.
- Crop out anything identifying: file names, note contents, and paths should
  be generic or fictional.
- Keep each image under ~500 KB; downscale rather than recompress hard.
