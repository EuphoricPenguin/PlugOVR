/// A double-ended queue. Unlike `VecDeque`, it is real-time safe and uses
/// a fixed block of memory passed in at initialization.
///
/// Implements a very small subset of the std::deque API for OddVoices.
pub struct Deque<T> {
    memory: Box<[T]>,
    capacity: usize,
    start: usize,
    size: usize,
    no_value: T,
}

impl<T: Copy + Default> Deque<T> {
    /// Create a new Deque from a boxed slice.
    /// `start` and `size` specify the initial position and number of elements.
    pub fn new(memory: Box<[T]>, start: usize, size: usize, no_value: T) -> Self {
        let capacity = memory.len();
        Self {
            memory,
            capacity,
            start: start % capacity,
            size: if size > capacity { capacity } else { size },
            no_value,
        }
    }

    /// Return the number of elements in the deque.
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Return true if the deque is empty.
    #[inline]
    pub fn empty(&self) -> bool {
        self.size == 0
    }

    /// Return the front element, or a copy of the no-value if empty.
    #[inline]
    pub fn front(&self) -> T {
        if self.empty() {
            self.no_value
        } else {
            self.memory[self.start]
        }
    }

    /// Return the element at the given index (0 = front), or the no-value if out of bounds.
    #[inline]
    pub fn get(&self, index: usize) -> T {
        if index >= self.size {
            return self.no_value;
        }
        let position = (self.start + index) % self.capacity;
        self.memory[position]
    }

    /// Append an element to the back. If the deque is full, the element is dropped.
    #[inline]
    pub fn push_back(&mut self, item: T) {
        if self.size >= self.capacity {
            return;
        }
        let end = (self.start + self.size) % self.capacity;
        self.memory[end] = item;
        self.size += 1;
    }

    /// Remove the front element. Does nothing if empty.
    #[inline]
    pub fn pop_front(&mut self) {
        if self.empty() {
            return;
        }
        self.size -= 1;
        self.start = (self.start + 1) % self.capacity;
    }

    /// Peek at the element at the given index without removing it.
    #[inline]
    pub fn peek(&self, index: usize) -> T {
        self.get(index)
    }
}