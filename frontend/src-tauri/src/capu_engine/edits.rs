use super::vocabulary::Action;

/// Applies a case-transform action to a single token. Ported verbatim from the
/// reference `convert_using_case()` in the model repo's `utils.py`. Non-case actions
/// return the token unchanged.
///
/// This is the single choke point through which every word passes — both the primary
/// per-word call in `apply_actions` and the neighbor-word call made when consuming a
/// `MergeSpace` pair — so it's also where we log unimplemented/unrecognized actions
/// rather than at the `apply_actions` call site (which would miss the merge-consumed
/// neighbor).
pub fn apply_case_transform(token: &str, action: Action) -> String {
    if matches!(
        action,
        Action::TransformVerbVbVbn | Action::TransformVerbVbVbc
    ) {
        log::warn!(
            "CAPU predicted an untranslated verb-transform action on word '{}' — leaving unchanged",
            token
        );
    } else if action == Action::Unknown {
        log::warn!(
            "CAPU predicted an unrecognized ($UNKNOWN) action on word '{}' — leaving unchanged",
            token
        );
    }

    match action {
        Action::TransformCaseLower => token.to_lowercase(),
        Action::TransformCaseUpper => token.to_uppercase(),
        Action::TransformCaseCapital => capitalize(token),
        Action::TransformCaseCapital1 => {
            // token[0] + token[1:].capitalize() in the Python reference:
            // first char untouched, capitalize() applied starting from the second char.
            let mut chars = token.chars();
            match chars.next() {
                Some(first) => {
                    let rest: String = chars.collect();
                    format!("{}{}", first, capitalize(&rest))
                }
                None => token.to_string(),
            }
        }
        Action::TransformCaseUpperMinus1 => {
            // token[:-1].upper() + token[-1] in the Python reference.
            let mut chars = token.chars();
            match chars.next_back() {
                Some(last) => {
                    let head: String = chars.collect();
                    format!("{}{}", head.to_uppercase(), last)
                }
                None => token.to_string(),
            }
        }
        _ => token.to_string(),
    }
}

/// Python's str.capitalize(): first char uppercase, rest lowercase.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let rest: String = chars.as_str().to_lowercase();
            format!("{}{}", first.to_uppercase(), rest)
        }
        None => String::new(),
    }
}

fn append_char(action: Action) -> Option<char> {
    match action {
        Action::AppendComma => Some(','),
        Action::AppendPeriod => Some('.'),
        Action::AppendColon => Some(':'),
        Action::AppendQuestion => Some('?'),
        _ => None,
    }
}

/// Applies one predicted `Action` per word to reconstruct the corrected word list.
/// `words.len()` must equal `actions.len()` — enforced unconditionally (not just in
/// debug builds), since a mismatch would otherwise either panic on out-of-bounds
/// indexing (too-short `actions`) or silently drop predicted actions (too-long
/// `actions`) with no signal as to which happened. `$TRANSFORM_VERB_*` actions are a
/// deliberate no-op (see plan notes) — this capu-finetuned model is not expected to
/// predict them; if it does, `apply_case_transform` logs and leaves the word unchanged
/// rather than guess.
pub fn apply_actions(words: &[String], actions: &[Action]) -> Vec<String> {
    assert_eq!(words.len(), actions.len());
    let mut output: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let mut word = apply_case_transform(&words[i], actions[i]);

        if actions[i] == Action::MergeSpace && i + 1 < words.len() {
            let next_word = apply_case_transform(&words[i + 1], actions[i + 1]);
            word.push_str(&next_word);
            if let Some(c) = append_char(actions[i + 1]) {
                word.push(c);
            }
            output.push(word);
            i += 2;
            continue;
        }

        if let Some(c) = append_char(actions[i]) {
            word.push(c);
        }
        output.push(word);
        i += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_leaves_words_unchanged() {
        let words = vec!["xin".to_string(), "chào".to_string()];
        let actions = vec![Action::Keep, Action::Keep];
        assert_eq!(apply_actions(&words, &actions), vec!["xin", "chào"]);
    }

    #[test]
    fn capital_uppercases_first_lowercases_rest() {
        assert_eq!(apply_case_transform("VIỆT", Action::TransformCaseCapital), "Việt");
    }

    #[test]
    fn capital_1_leaves_first_char_untouched() {
        // token[0] + token[1:].capitalize() -> first char kept as-is, second char capitalized
        assert_eq!(
            apply_case_transform("nội", Action::TransformCaseCapital1),
            "nỘi"
        );
    }

    #[test]
    fn upper_minus_1_leaves_last_char_untouched() {
        assert_eq!(
            apply_case_transform("viet", Action::TransformCaseUpperMinus1),
            "VIEt"
        );
    }

    #[test]
    fn append_period_suffixes_the_word() {
        let words = vec!["xong".to_string()];
        let actions = vec![Action::AppendPeriod];
        assert_eq!(apply_actions(&words, &actions), vec!["xong."]);
    }

    #[test]
    fn capitalize_and_append_compose_on_same_word() {
        let words = vec!["chào".to_string(), "bạn".to_string()];
        let actions = vec![Action::TransformCaseCapital, Action::AppendQuestion];
        assert_eq!(apply_actions(&words, &actions), vec!["Chào", "bạn?"]);
    }

    #[test]
    fn merge_space_joins_word_with_next_and_applies_next_actions() {
        let words = vec!["hôm".to_string(), "nay".to_string(), "đẹp".to_string()];
        // MergeSpace on "hôm" merges it with "nay" (which itself gets a period appended)
        let actions = vec![Action::MergeSpace, Action::AppendPeriod, Action::Keep];
        assert_eq!(apply_actions(&words, &actions), vec!["hômnay.", "đẹp"]);
    }

    #[test]
    fn verb_transform_actions_are_untranslated_noop() {
        let words = vec!["đi".to_string()];
        let actions = vec![Action::TransformVerbVbVbn];
        assert_eq!(apply_actions(&words, &actions), vec!["đi"]);
    }

    #[test]
    fn verb_transform_on_merge_consumed_neighbor_is_still_a_noop() {
        // Regression test: actions[i+1] reaches apply_case_transform via the
        // MergeSpace branch's neighbor call, not just the top-of-loop primary call.
        let words = vec!["đi".to_string(), "làm".to_string()];
        let actions = vec![Action::MergeSpace, Action::TransformVerbVbVbn];
        assert_eq!(apply_actions(&words, &actions), vec!["đilàm"]);
    }

    #[test]
    fn merge_space_on_last_word_has_no_neighbor_to_merge_with() {
        let words = vec!["một".to_string()];
        let actions = vec![Action::MergeSpace];
        assert_eq!(apply_actions(&words, &actions), vec!["một"]);
    }

    #[test]
    fn empty_input_produces_empty_output() {
        let words: Vec<String> = vec![];
        let actions: Vec<Action> = vec![];
        assert_eq!(apply_actions(&words, &actions), Vec::<String>::new());
    }

    #[test]
    #[should_panic]
    fn mismatched_lengths_panic_in_all_build_profiles() {
        let words = vec!["chỉ".to_string(), "một".to_string()];
        let actions = vec![Action::Keep];
        apply_actions(&words, &actions);
    }
}
