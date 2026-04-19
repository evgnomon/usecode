# C++ Safety Tips

Common pitfalls to watch for during development and code review.

---

## 1. `string_view` Returned from a Function

A `string_view` does not own the string. If the underlying data is destroyed or moved,
the view dangles silently.

```cpp
// BAD — caller holds a view into a member that can be moved/destroyed
class Session {
    std::string token_;
public:
    std::string_view token() const { return token_; }
};

auto sv = session.token();
session = Session{};  // token_ destroyed — sv now dangles
std::cout << sv;      // undefined behavior
```

```cpp
// GOOD — return a copy, or document lifetime requirements
std::string token() const { return token_; }
```

Also watch for virtual interfaces that return `string_view` — an override might
return a view into a temporary without the caller realizing it.

---

## 2. Chained Call in a Range-for

Before C++23, only the outermost temporary in a range-for header is lifetime-extended.
Intermediates are destroyed before the loop body runs.

```cpp
// BAD — get_names() temporary destroyed before loop body
std::vector<std::string> get_names();

for (auto& n : get_names() | std::views::take(3)) {
    std::cout << n;  // dangling reference
}
```

```cpp
// GOOD — materialize the source first
auto names = get_names();
for (auto& n : names | std::views::take(3)) {
    std::cout << n;
}
```

Piping views on an **lvalue** is well-defined — the source outlives the loop and the
`filter_view` temporary is lifetime-extended by the range-for:

```cpp
// GOOD — people is an lvalue; filter_view just holds a reference to it
std::vector<Person> people = {{"Alice", 30}, {"Bob", 15}, {"Carol", 22}};

for (auto& [name, age] : people
     | std::views::filter([](auto& p) { return p.age >= 18; }))
{
    std::cout << std::format("{} is {}\n", name, age);
}
```

Be cautious when refactoring: replacing the lvalue with a function call
(e.g. `get_people() | views::filter(…)`) silently introduces a dangling temporary.

---

## 3. Mutation Inside Iteration

Inserting or erasing from a container while iterating invalidates iterators.

```cpp
// BAD — push_back may reallocate, invalidating the range-for iterator
std::vector<int> v{1, 2, 3};
for (auto x : v) {
    if (x == 2) v.push_back(4);  // undefined behavior
}
```

```cpp
// GOOD — collect mutations, apply after the loop
std::vector<int> to_add;
for (auto x : v) {
    if (x == 2) to_add.push_back(4);
}
v.insert(v.end(), to_add.begin(), to_add.end());
```

For erasure, prefer the erase-remove idiom or `std::erase_if` (C++20).

---

## 4. Uninitialized Variables

Aggregates with no default member initializers allow indeterminate values.

```cpp
// BAD — default construction leaves x, y indeterminate
struct Vec2 {
    double x, y;
};

Vec2 v;
std::cout << v.x;  // undefined behavior
```

```cpp
// GOOD — provide default member initializers
struct Vec2 {
    double x = 0.0;
    double y = 0.0;
};
```

This also applies to local scalars:

```cpp
int count;        // BAD  — indeterminate
int count = 0;    // GOOD
```

---

## 5. `[]` Without Bounds Validation

`operator[]` on standard containers and raw arrays performs no bounds check.

```cpp
// BAD — silent out-of-bounds read/write
std::vector<int> v{1, 2, 3};
int x = v[5];  // undefined behavior
```

```cpp
// GOOD — use .at() for runtime bounds checking
int x = v.at(5);  // throws std::out_of_range
```

For custom containers, add a debug-mode assertion:

```cpp
T& operator[](size_t i) {
    assert(i < size_ && "index out of bounds");
    return data_[i];
}
```

For computed indices (e.g. `data_[r * cols_ + c]`), validate both dimensions
before the arithmetic to prevent silent buffer overruns.
