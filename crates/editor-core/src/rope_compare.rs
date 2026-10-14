use ropey::Rope;

struct Walk {
    edge: for<'a> fn(&'a [u8], usize) -> &'a [u8],
    rest: for<'a> fn(&'a [u8], usize) -> &'a [u8],
    common: fn(&[u8], &[u8]) -> usize,
}

const FORWARD: Walk = Walk {
    edge: |bytes, len| &bytes[..len],
    rest: |bytes, len| &bytes[len..],
    common: |old, new| old.iter().zip(new).position(|(old, new)| old != new).unwrap_or(old.len()),
};

const BACKWARD: Walk = Walk {
    edge: |bytes, len| &bytes[bytes.len() - len..],
    rest: |bytes, len| &bytes[..bytes.len() - len],
    common: |old, new| old.iter().rev().zip(new.iter().rev()).position(|(old, new)| old != new).unwrap_or(old.len()),
};

pub(crate) fn common_prefix_bytes(old: &Rope, new: &Rope) -> usize {
    common_len(old.chunks().map(str::as_bytes), new.chunks().map(str::as_bytes), &FORWARD)
}

pub(crate) fn common_suffix_bytes(old: &Rope, new: &Rope) -> usize {
    common_len(reversed_chunks(old), reversed_chunks(new), &BACKWARD)
}

fn reversed_chunks(rope: &Rope) -> impl Iterator<Item = &[u8]> {
    rope.chunks_at_byte(rope.len_bytes()).0.reversed().map(str::as_bytes)
}

fn common_len<'a>(
    mut old_chunks: impl Iterator<Item = &'a [u8]>,
    mut new_chunks: impl Iterator<Item = &'a [u8]>,
    walk: &Walk,
) -> usize {
    let mut old_bytes: &[u8] = &[];
    let mut new_bytes: &[u8] = &[];
    let mut matched = 0;
    loop {
        if old_bytes.is_empty() {
            let Some(chunk) = old_chunks.next() else {
                return matched;
            };
            old_bytes = chunk;
            continue;
        }
        if new_bytes.is_empty() {
            let Some(chunk) = new_chunks.next() else {
                return matched;
            };
            new_bytes = chunk;
            continue;
        }
        let shared = old_bytes.len().min(new_bytes.len());
        let (old_edge, new_edge) = ((walk.edge)(old_bytes, shared), (walk.edge)(new_bytes, shared));
        let common = if old_edge == new_edge { shared } else { (walk.common)(old_edge, new_edge) };
        matched += common;
        if common < shared {
            return matched;
        }
        old_bytes = (walk.rest)(old_bytes, shared);
        new_bytes = (walk.rest)(new_bytes, shared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_and_suffix_walk_across_chunk_boundaries() {
        let text = "abc\n".repeat(5000);
        let old = Rope::from_str(&text);
        let mut new = old.clone();
        new.insert(15_000, "Z");
        assert_eq!(common_prefix_bytes(&old, &new), 15_000);
        assert_eq!(common_suffix_bytes(&old, &new), 5_000);
        assert_eq!(common_prefix_bytes(&old, &old.clone()), old.len_bytes());
        assert_eq!(common_suffix_bytes(&old, &old.clone()), old.len_bytes());
    }

    #[test]
    fn a_shorter_rope_bounds_the_match() {
        let (old, new) = (Rope::from_str("ab"), Rope::from_str("xab"));
        assert_eq!(common_prefix_bytes(&old, &new), 0);
        assert_eq!(common_suffix_bytes(&old, &new), 2);
    }
}
