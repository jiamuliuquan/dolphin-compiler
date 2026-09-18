pkg util;

pub trait Head {
    type Item;
    fn head(self: *const Self): Self::Item;
}

pub struct Pair<T> {
    pub first: T,
    pub second: T,
}

impl<T> Pair<T> {
    pub fn swapped(self): Pair<T> {
        return Pair<T>(self.second, self.first);
    }
}

impl<T> Head for Pair<T> {
    type Item = T;
    fn head(self: *const Self): T {
        return self->first;
    }
}
