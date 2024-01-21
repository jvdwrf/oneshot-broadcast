#[derive(Debug)]
pub(crate) struct OptVec<T> {
    vec: Vec<Option<T>>,
    first_free: Option<usize>,
}

impl<T> OptVec<T> {
    pub(crate) fn new() -> Self {
        Self {
            vec: Vec::new(),
            first_free: None,
        }
    }

    pub(crate) fn get_mut(&mut self, pos: usize) -> Option<&mut Option<T>> {
        self.vec.get_mut(pos)
    }

    pub(crate) fn insert(&mut self, pos: usize, value: T) -> Option<T> {
        if pos >= self.vec.len() {
            self.vec.resize_with(pos + 1, || None);
        }

        match self.vec[pos].replace(value) {
            Some(value) => Some(value),
            None => {
                if self.first_free == Some(pos) {
                    self.first_free = self.find_first_free();
                }
                None
            }
        }
    }

    pub(crate) fn remove(&mut self, pos: usize) -> Option<T> {
        if pos >= self.vec.len() {
            None
        } else {
            if let Some(first_free) = self.first_free {
                if pos < first_free {
                    self.first_free = Some(pos);
                }
            } else {
                self.first_free = Some(pos);
            }
            self.vec[pos].take()
        }
    }

    pub(crate) fn add(&mut self, value: T) -> usize {
        if let Some(pos) = self.first_free {
            self.insert(pos, value);
            pos
        } else {
            let pos = self.vec.len();
            self.vec.push(Some(value));
            pos
        }
    }

    fn find_first_free(&self) -> Option<usize> {
        self.vec
            .iter()
            .enumerate()
            .find(|(_, opt)| opt.is_none())
            .map(|(pos, _)| pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let opt_vec: OptVec<i32> = OptVec::new();
        assert_eq!(opt_vec.vec.len(), 0);
        assert_eq!(opt_vec.first_free, None);
    }

    #[test]
    fn test_get_mut() {
        let mut opt_vec: OptVec<i32> = OptVec::new();
        opt_vec.insert(0, 10);
        opt_vec.insert(1, 20);
        opt_vec.insert(2, 30);

        let value = opt_vec.get_mut(1);
        assert_eq!(value, Some(&mut Some(20)));
    }

    #[test]
    fn test_insert() {
        let mut opt_vec: OptVec<i32> = OptVec::new();
        let value = opt_vec.insert(0, 10);
        assert_eq!(value, None);
        assert_eq!(opt_vec.vec[0], Some(10));

        let value = opt_vec.insert(1, 20);
        assert_eq!(value, None);
        assert_eq!(opt_vec.vec[1], Some(20));

        let value = opt_vec.insert(0, 30);
        assert_eq!(value, Some(10));
        assert_eq!(opt_vec.vec[0], Some(30));
    }

    #[test]
    fn test_remove() {
        let mut opt_vec: OptVec<i32> = OptVec::new();
        opt_vec.insert(0, 10);
        opt_vec.insert(1, 20);
        opt_vec.insert(2, 30);

        let value = opt_vec.remove(1);
        assert_eq!(value, Some(20));
        assert_eq!(opt_vec.vec[1], None);
        assert_eq!(opt_vec.first_free, Some(1));

        let value = opt_vec.remove(0);
        assert_eq!(value, Some(10));
        assert_eq!(opt_vec.vec[0], None);
        assert_eq!(opt_vec.first_free, Some(0));

        let value = opt_vec.remove(2);
        assert_eq!(value, Some(30));
        assert_eq!(opt_vec.vec[2], None);
        assert_eq!(opt_vec.first_free, Some(0));
    }

    #[test]
    fn test_add() {
        let mut opt_vec: OptVec<i32> = OptVec::new();
        let pos1 = opt_vec.add(10);
        assert_eq!(pos1, 0);
        assert_eq!(opt_vec.vec[0], Some(10));
        assert_eq!(opt_vec.first_free, None);

        let pos2 = opt_vec.add(20);
        assert_eq!(pos2, 1);
        assert_eq!(opt_vec.vec[1], Some(20));
        assert_eq!(opt_vec.first_free, None);

        opt_vec.remove(0);
        let pos3 = opt_vec.add(30);
        assert_eq!(pos3, 0);
    }

    #[test]
    fn test_first_free() {
        let mut opt_vec: OptVec<i32> = OptVec::new();
        opt_vec.insert(0, 10);
        opt_vec.insert(1, 20);
        opt_vec.insert(2, 30);

        let first_free = opt_vec.find_first_free();
        assert_eq!(first_free, None);

        opt_vec.remove(1);
        let first_free = opt_vec.find_first_free();
        assert_eq!(first_free, Some(1));

        opt_vec.remove(0);
        let first_free = opt_vec.find_first_free();
        assert_eq!(first_free, Some(0));

        opt_vec.remove(2);
        let first_free = opt_vec.find_first_free();
        assert_eq!(first_free, Some(0));

        opt_vec.insert(0, 40);
        let first_free = opt_vec.find_first_free();
        assert_eq!(first_free, Some(1));

        opt_vec.insert(2, 50);
        let first_free = opt_vec.find_first_free();
        assert_eq!(first_free, Some(1));
    }


    #[test]
    fn test_insert_past_last_value() {
        let mut opt_vec: OptVec<i32> = OptVec::new();
        opt_vec.insert(0, 10);
        opt_vec.insert(2, 30);

        assert_eq!(opt_vec.vec.len(), 3);
        assert_eq!(opt_vec.vec[0], Some(10));
        assert_eq!(opt_vec.vec[1], None);
        assert_eq!(opt_vec.vec[2], Some(30));
    }
}
