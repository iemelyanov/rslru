use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::mem;
use std::ptr::NonNull;

struct Item<K, V> {
    key: K,
    val: V,
    next: Option<NonNull<Item<K, V>>>,
    prev: Option<NonNull<Item<K, V>>>,
}

struct List<K, V> {
    head: Option<NonNull<Item<K, V>>>,
    tail: Option<NonNull<Item<K, V>>>,
    marker: PhantomData<Box<Item<K, V>>>,
}

struct Internal<K, V> {
    map: HashMap<KeyRef<K>, NonNull<Item<K, V>>>,
    items: List<K, V>,
    max_len: usize,
}

pub struct LRU<K, V> {
    internal: RefCell<Internal<K, V>>,
}

impl<K, V> Item<K, V> {
    fn new(key: K, val: V) -> Self {
        Self {
            key: key,
            val: val,
            next: None,
            prev: None,
        }
    }
}

impl<K, V> List<K, V> {
    fn new() -> Self {
        Self {
            head: None,
            tail: None,
            marker: PhantomData,
        }
    }

    fn push_front(&mut self, key: K, val: V) -> NonNull<Item<K, V>> {
        unsafe {
            let item = NonNull::new_unchecked(Box::leak(Box::new(Item::new(key, val))));
            (*item.as_ptr()).next = self.head;
            match self.head {
                None => self.tail = Some(item),
                Some(head) => (*head.as_ptr()).prev = Some(item),
            }
            self.head = Some(item);

            item
        }
    }

    fn move_to_front(&mut self, item: NonNull<Item<K, V>>) {
        unsafe {
            if (*item.as_ref()).prev.is_none() {
                return;
            }

            self.tail.map(|tail| {
                if tail == item {
                    self.tail = (*item.as_ptr()).prev;
                }
            });

            (*item.as_ptr()).prev.map(|prev| {
                (*prev.as_ptr()).next = (*item.as_ptr()).next;
            });

            (*item.as_ptr()).next.map(|next| {
                (*next.as_ptr()).prev = (*item.as_ptr()).prev;
            });

            (*item.as_ptr()).next = self.head;
            (*item.as_ptr()).prev = None;
            self.head.map(|head| {
                (*head.as_ptr()).prev = Some(item);
            });
            self.head = Some(item);
        }
    }

    fn pop_back(&mut self) -> Option<NonNull<Item<K, V>>> {
        self.tail.map(|tail| unsafe {
            self.tail = None;
            (*tail.as_ptr()).prev.map(|prev| {
                (*prev.as_ptr()).next = None;
                self.tail = Some(prev);
            });
            tail
        })
    }
}

struct KeyRef<K> {
    key: *const K,
}

impl<K> KeyRef<K> {
    fn new(key: &K) -> Self {
        Self { key: key }
    }
}

impl<K: Hash> Hash for KeyRef<K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe {
            (*self.key).hash(state);
        }
    }
}

impl<K: PartialEq> PartialEq for KeyRef<K> {
    fn eq(&self, other: &KeyRef<K>) -> bool {
        unsafe { (*self.key).eq(&*other.key) }
    }
}

impl<K: Eq> Eq for KeyRef<K> {}

impl<K, V> Internal<K, V> {
    fn new(len: usize) -> Self {
        Self {
            map: HashMap::with_capacity(len + 1),
            items: List::new(),
            max_len: len,
        }
    }

    fn put(&mut self, key: K, val: V) -> Option<V>
    where
        K: Hash + Eq,
    {
        if let Some(item) = self.map.get_mut(&KeyRef::new(&key)) {
            let mut val = val;
            unsafe {
                mem::swap(&mut (*item).as_mut().val, &mut val);
            };
            self.items.move_to_front(*item);
            return Some(val);
        }

        if self.map.len() >= self.max_len {
            self.items.pop_back().map(|item| unsafe {
                self.map.remove(&KeyRef::new(&item.as_ref().key));
                Box::from_raw(item.as_ptr());
            });
        }

        let item = self.items.push_front(key, val);
        unsafe {
            self.map.insert(KeyRef::new(&item.as_ref().key), item);
        }

        None
    }

    fn get_item(&mut self, key: &K) -> Option<NonNull<Item<K, V>>>
    where
        K: Hash + Eq,
    {
        if let Some(item) = self.map.get_mut(&KeyRef::new(key)) {
            self.items.move_to_front(*item);
            return Some(*item);
        }

        None
    }
}

impl<K, V> Drop for Internal<K, V> {
    fn drop(&mut self) {
        while let Some(item) = self.items.pop_back() {
            Box::from(item.as_ptr());
        }
    }
}

impl<K, V> LRU<K, V> {
    pub fn new(len: usize) -> Self {
        Self {
            internal: RefCell::new(Internal::new(len)),
        }
    }

    pub fn put(&mut self, key: K, val: V) -> Option<V>
    where
        K: Hash + Eq,
    {
        self.internal.borrow_mut().put(key, val)
    }

    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: Hash + Eq,
    {
        self.internal
            .borrow_mut()
            .get_item(key)
            .map(|item| unsafe { &item.as_ref().val })
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V>
    where
        K: Hash + Eq,
    {
        self.internal
            .borrow_mut()
            .get_item(key)
            .map(|mut item| unsafe { &mut item.as_mut().val })
    }

    pub fn count(&self) -> usize {
        self.internal.borrow().map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        let mut list = List::new();
        let item = list.push_front("k1", "v1");
        list.push_front("k2", "v2");
        list.push_front("k3", "v3");

        list.move_to_front(item);

        let mut item: Option<NonNull<Item<_, _>>>;
        unsafe {
            item = list.pop_back();
            assert_eq!("k2", item.unwrap().as_ref().key);

            item = list.pop_back();
            assert_eq!("k3", item.unwrap().as_ref().key);

            item = list.pop_back();
            assert_eq!("k1", item.unwrap().as_ref().key);

            assert_eq!(None, list.pop_back());
        }

        let mut lru = LRU::new(3);
        lru.put("key1", "val1");
        lru.put("key2", "val2");
        lru.put("key3", "val3");

        assert_eq!(3, lru.count());

        assert_eq!(Some(&"val1"), lru.get(&"key1"));
        assert_eq!(Some(&"val2"), lru.get(&"key2"));
        assert_eq!(Some(&"val3"), lru.get(&"key3"));
        assert_eq!(None, lru.get(&"key4"));

        assert_eq!(Some(&mut "val1"), lru.get_mut(&"key1"));
        assert_eq!(Some(&mut "val2"), lru.get_mut(&"key2"));
        assert_eq!(Some(&mut "val3"), lru.get_mut(&"key3"));
        assert_eq!(None, lru.get_mut(&"key4"));

        lru.put("key4", "val4");
        assert_eq!(3, lru.count());
        assert_eq!(Some(&"val2"), lru.get(&"key2"));
        assert_eq!(Some(&"val3"), lru.get(&"key3"));
        assert_eq!(Some(&"val4"), lru.get(&"key4"));

        lru = LRU::new(0);
        lru.put("key1", "val1");
        lru.put("key2", "val2");

        assert_eq!(1, lru.count());
        assert_eq!(None, lru.get(&"key1"));
        assert_eq!(Some(&"val2"), lru.get(&"key2"));
    }
}
