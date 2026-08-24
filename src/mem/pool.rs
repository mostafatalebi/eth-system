

pub struct PoolMember<T> {
    is_free: bool,
    val: T
}

impl<T> PoolMember<T> {
    fn new(is_free: bool, val: T) -> Self {
        Self { is_free, val }
    }

    pub fn get_value(&mut self) -> &mut T {
        &mut self.val
    }
}

/// Not thread-safe
pub struct MemPool<T> {
    pool: Vec<PoolMember<T>>,
    free_count: usize,
    next_free_index: usize,
    free_index_list: Vec<usize>
}

impl<T> MemPool<T> {
    pub fn new<F: Fn() -> T>(size: usize, closure: F) -> Self {
        let mut s = Self {
            pool: Vec::with_capacity(size),
            free_count: size,
            next_free_index: 0,
            free_index_list: Vec::with_capacity(size),
        };

        for i in 0..size {
            s.pool.push(PoolMember::new(true, closure()));
            s.free_index_list.push(i);
        }
        return s
    }

    /// returns a mutable reference to the underlying T
    /// wrapped in PoolMember type. When done and the caller
    /// aims to put back the object for re-use, it simply
    /// needs to call release().
    /// if the pool is full and resources are not available,
    /// None is returned.
    pub fn acquire(&mut self) -> Option<*mut PoolMember<T>> {
        if self.free_count > 0 {
            self.free_count -= 1;
            let free = self.free_index_list[self.next_free_index];
            if self.pool[free].is_free {
                self.next_free_index += 1;
                self.pool[free].is_free = false;
                return Some(unsafe { std::ptr::addr_of_mut!((*self.pool.as_mut_ptr().add(free))) });
            }
        }
        None
    }

    pub fn release(&mut self, obj: *mut PoolMember<T>) {
        if self.next_free_index > 0 && unsafe { !(*obj).is_free } {
            let base_addr = self.pool.as_mut_ptr();
            let index = unsafe { obj.offset_from(base_addr) as usize };
            self.free_index_list[self.next_free_index - 1] = index;
            self.next_free_index -= 1;
            self.free_count += 1;
            unsafe { (*obj).is_free = true }
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    struct TestObj {
        field: String
    }
    #[test]
    fn test_mempool() {
        let size = 3;
        let mut mem_pool = MemPool::<TestObj>::new(size, || TestObj{field: "uninitialized".to_string()});
        let r1 = mem_pool.acquire();
        assert_eq!(mem_pool.free_count, 2);
        assert_eq!(r1.is_some(), true);
        let r2 = mem_pool.acquire();
        assert_eq!(mem_pool.free_count, 1);
        assert_eq!(r2.is_some(), true);
        let r3 = mem_pool.acquire();
        assert_eq!(mem_pool.free_count, 0);
        assert_eq!(r2.is_some(), true);
        let r4 = mem_pool.acquire();
        assert_eq!(r4.is_some(), false);
        assert_eq!(mem_pool.free_count.clone(), 0);
        assert_eq!(mem_pool.next_free_index.clone(), size);
        let r = mem_pool.release(r3.unwrap());
        assert_eq!(mem_pool.free_count, 1);
        assert_eq!(mem_pool.next_free_index, 2);
        let r = mem_pool.release(r2.unwrap());
        assert_eq!(mem_pool.free_count, 2);
        assert_eq!(mem_pool.next_free_index, 1);
        let r = mem_pool.release(r1.unwrap());
        assert_eq!(mem_pool.free_count, 3);
        assert_eq!(mem_pool.next_free_index, 0);

        // releasing an already released object
        // must have no effect
        let r = mem_pool.release(r1.unwrap());
        assert_eq!(mem_pool.free_count, 3);
        assert_eq!(mem_pool.next_free_index, 0);
    }
}
