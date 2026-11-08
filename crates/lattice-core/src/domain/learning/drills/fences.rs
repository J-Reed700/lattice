//! The fenced blocks of a text, read the way a markdown reader reads them.

/// One fenced block: its opening fence line through its closing one, or through the end of the text when it is never
/// closed.
pub(super) struct FencedBlock<'a> {
    /// The byte where the opening fence line starts.
    pub(super) start: usize,
    /// The byte where the last line of the block ends, before its line break.
    pub(super) end: usize,
    /// What follows the opening fence's marks on its line.
    pub(super) info: &'a str,
}

/// The fenced blocks of `text`, in order. A fence is a line of three or more backticks or tildes indented at most three
/// spaces. With no block open, a fence opens one; the next fence of the same mark closes it, and a fence of the other
/// mark inside it is code.
pub(super) fn fenced_blocks(text: &str) -> Vec<FencedBlock<'_>> {
    let mut blocks = Vec::new();
    let mut open: Option<(char, usize, &str)> = None;
    let mut offset = 0;
    let mut last_end = 0;
    for raw in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw.len();
        let line = raw
            .strip_suffix('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .unwrap_or(raw);
        last_end = line_start + line.len();
        let fence = line.trim_start_matches(' ');
        if line.len() - fence.len() > 3 {
            continue;
        }
        let Some(mark) = fence
            .chars()
            .next()
            .filter(|&mark| mark == '`' || mark == '~')
        else {
            continue;
        };
        let info = fence.trim_start_matches(mark);
        if fence.len() - info.len() < 3 {
            continue;
        }
        match open {
            Some((opened, start, opening_info)) if opened == mark => {
                blocks.push(FencedBlock {
                    start,
                    end: last_end,
                    info: opening_info,
                });
                open = None;
            }
            Some(_) => {}
            None => open = Some((mark, line_start, info)),
        }
    }
    if let Some((_, start, info)) = open {
        blocks.push(FencedBlock {
            start,
            end: last_end,
            info,
        });
    }
    blocks
}
