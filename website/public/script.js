// Every screenshot remains a normal full-resolution image link when JavaScript is unavailable.
document.querySelectorAll("[data-gallery]").forEach((gallery) => {
  const options = [...gallery.querySelectorAll(".showcase-tabs button")];
  const image = gallery.querySelector(".showcase-image");
  const link = gallery.querySelector(".showcase-zoom");
  const caption = gallery.querySelector(".showcase-caption");
  const meta = gallery.querySelector(".showcase-meta");

  options.forEach((option, index) => {
    option.addEventListener("click", () => {
      const source = `./assets/${option.dataset.image}.png`;
      options.forEach((button) =>
        button.setAttribute("aria-pressed", String(button === option)),
      );
      image.src = source;
      image.alt = option.dataset.alt;
      image.width = Number(option.dataset.width);
      image.height = Number(option.dataset.height);
      link.href = source;
      link.setAttribute(
        "aria-label",
        `Open ${option.textContent.trim()} screenshot at full resolution`,
      );
      caption.textContent = option.dataset.caption;
      if (meta && option.dataset.meta) {
        meta.textContent = option.dataset.meta;
      }
    });

    option.addEventListener("keydown", (event) => {
      let next;
      if (event.key === "ArrowRight") next = (index + 1) % options.length;
      if (event.key === "ArrowLeft")
        next = (index - 1 + options.length) % options.length;
      if (event.key === "Home") next = 0;
      if (event.key === "End") next = options.length - 1;
      if (next === undefined) return;
      event.preventDefault();
      options[next].focus();
      options[next].click();
    });
  });
});

const dialog = document.querySelector(".image-dialog");
if (dialog) {
  const dialogImage = dialog.querySelector(".dialog-image");
  const dialogTitle = dialog.querySelector("#image-dialog-title");
  let opener;

  document.querySelectorAll("[data-zoom]").forEach((link) => {
    link.addEventListener("click", (event) => {
      if (
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey ||
        typeof dialog.showModal !== "function"
      ) {
        return;
      }
      event.preventDefault();
      opener = link;
      const image = link.querySelector("img");
      dialogImage.src = link.href;
      dialogImage.alt = image ? image.alt : "";
      dialogTitle.textContent =
        link.dataset.caption || (image ? image.alt : "Lattice screenshot");
      dialog.showModal();
    });
  });

  dialog
    .querySelector(".dialog-close")
    ?.addEventListener("click", () => dialog.close());

  dialog.addEventListener("click", (event) => {
    if (event.target !== dialog) return;
    const bounds = dialog.getBoundingClientRect();
    if (
      event.clientX < bounds.left ||
      event.clientX > bounds.right ||
      event.clientY < bounds.top ||
      event.clientY > bounds.bottom
    ) {
      dialog.close();
    }
  });

  dialog.addEventListener("close", () =>
    opener?.focus({ preventScroll: true }),
  );
}
