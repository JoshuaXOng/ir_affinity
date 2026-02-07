use std::collections::HashSet;

pub fn hashset_to_mask(selections: &HashSet<usize>) -> usize {
    let mut mask = 0usize;
    for selection in selections {
        mask |= 1usize << selection;
    }
    mask
}

pub fn mask_to_hashset(mask: &usize) -> HashSet<usize> {
    let mut hashset = HashSet::new();
    for bit_index in 0..usize::BITS {
        let is_selected = (1usize << bit_index) & mask != 0; 
        if is_selected {
            hashset.insert(bit_index as usize);
        } 
    }
    hashset
}
