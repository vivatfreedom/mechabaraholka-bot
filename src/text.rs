pub fn split_addword_args(input: &str) -> Vec<String> {
    input
        .split(|ch| ch == ',' || ch == ';' || ch == ' ')
        .map(|word| word.trim().to_lowercase())
        .filter(|word| !word.is_empty())
        .collect()
}

pub fn contains_ban_word(text: &str, words: &[String]) -> bool {
    let text = text.to_lowercase();
    words
        .iter()
        .map(|word| word.to_lowercase())
        .any(|word| text.contains(&word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_addword_args_accepts_commas_semicolons_and_spaces() {
        assert_eq!(
            split_addword_args("Spam,Scam; Bad  Words"),
            vec!["spam", "scam", "bad", "words"]
        );
    }

    #[test]
    fn split_addword_args_drops_empty_parts() {
        assert_eq!(split_addword_args(" , ; spam ;; "), vec!["spam"]);
    }

    #[test]
    fn contains_ban_word_uses_case_insensitive_substring_matching() {
        let words = vec!["spam".to_string(), "fraud".to_string()];
        assert!(contains_ban_word("This has SPAM inside", &words));
        assert!(!contains_ban_word("Clean message", &words));
    }
}
