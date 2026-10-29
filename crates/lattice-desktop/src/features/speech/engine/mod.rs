//! Engines behind read-aloud: the Kokoro voice and the G2P that feeds it.

pub mod g2p;
pub mod kokoro;
mod service;

pub use service::{Clip, KokoroSpeech, SPEECH_IDLE_TTL};

#[cfg(test)]
mod tests {
    use super::service::split_phonemes;

    #[test]
    fn long_phonemes_break_at_sentences_then_spaces() {
        assert_eq!(split_phonemes("ab cd. ef gh", 8), ["ab cd.", "ef gh"]);
        assert_eq!(split_phonemes("abc def ghi", 8), ["abc def", "ghi"]);
        assert_eq!(split_phonemes("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(split_phonemes("  ", 4), Vec::<String>::new());
    }
}
