namespace oddvoices {

/// A double-ended queue. Unlike std::deque, it is real-time safe and uses
/// a fixed block of memory passed in at initialization.
///
/// Implements a very small subset of the std::deque API for OddVoices.
template <typename T>
class Deque {
public:
    Deque(T* memory, int capacity, int start, int size, T noValue);

    int size() { return m_size; };
    bool empty() { return m_size == 0; };
    T front();
    T operator[](int);
    void push_back(T);
    void pop_front();

private:
    T* const m_memory;
    const int m_capacity;
    int m_start;
    int m_size;
    T m_noValue;
};

template <typename T>
Deque<T>::Deque(T* memory, int capacity, int start, int size, T noValue)
    : m_memory(memory)
    , m_capacity(capacity)
    , m_start(start)
    , m_size(size)
    , m_noValue(noValue)
{
}

template <typename T>
void Deque<T>::push_back(T item)
{
    if (m_size >= m_capacity) {
        return;
    }
    int end = m_start + m_size;
    if (end > m_capacity) {
        end -= m_capacity;
    }
    m_memory[end] = item;
    m_size += 1;
}

template <typename T>
void Deque<T>::pop_front()
{
    if (empty()) {
        return;
    }
    m_size -= 1;
    m_start += 1;
    if (m_start == m_capacity) {
        m_start = 0;
    }
}

template <typename T>
T Deque<T>::front()
{
    if (empty()) {
        return m_noValue;
    }
    return m_memory[m_start];
}

template <typename T>
T Deque<T>::operator[](int index)
{
    if (index >= m_size) {
        return m_noValue;
    }
    int position = m_start + index;
    if (position > m_capacity) {
        position -= m_capacity;
    }
    return m_memory[position];
}

} // namespace oddvoices
