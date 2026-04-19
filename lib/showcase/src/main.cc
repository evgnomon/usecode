// ============================================================================
// C++ COMPREHENSIVE FEATURE SHOWCASE
// ============================================================================
// A single-file tour of C++ features from C++98 through C++23.
// Compile: g++ -std=c++23 -fconcepts -pthread -o showcase cpp_showcase.cc
// (Use -std=c++20 if C++23 is unavailable; some sections are guarded.)
// ============================================================================

#include "main.hh"

#include <iostream>
#include <string>
#include <string_view>
#include <vector>
#include <array>
#include <map>
#include <unordered_map>
#include <set>
#include <list>
#include <deque>
#include <queue>
#include <stack>
#include <tuple>
#include <optional>
#include <variant>
#include <any>
#include <functional>
#include <algorithm>
#include <numeric>
#include <ranges>
#include <memory>
#include <utility>
#include <type_traits>
#include <concepts>
#include <coroutine>
#include <span>
#include <bitset>
#include <format>
#include <thread>
#include <mutex>
#include <shared_mutex>
#include <atomic>
#include <future>
#include <condition_variable>
#include <semaphore>
#include <latch>
#include <barrier>
#include <chrono>
#include <random>
#include <regex>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <compare>
#include <numbers>
#include <source_location>
#include <bit>
#include <cmath>
#include <cassert>
#include <cstdint>
#include <cstring>
#include <initializer_list>
#include <stdexcept>
#include <typeinfo>
#include <typeindex>

namespace fs = std::filesystem;
using namespace std::chrono_literals;
using namespace std::string_literals;
using namespace std::string_view_literals;

// ============================================================================
// SECTION 0: Utility — Banner Printer
// ============================================================================
static int section_counter = 0;

void banner(std::string_view title) {
    std::cout << "\n╔══════════════════════════════════════════════════════════════╗\n";
    std::cout << std::format("║  SECTION {:>2}: {:<49}║\n", section_counter++, title);
    std::cout << "╚══════════════════════════════════════════════════════════════╝\n";
}

void sub(std::string_view title) {
    std::cout << std::format("\n  ── {} ──\n", title);
}

// ============================================================================
// SECTION 1: Fundamental Types & Literals
// ============================================================================
void showcase_fundamentals() {
    banner("Fundamental Types & Literals");

    // Integer types
    [[maybe_unused]] int8_t   i8  = -128;
    [[maybe_unused]] uint8_t  u8  = 255;
    [[maybe_unused]] int16_t  i16 = -32'768;
    [[maybe_unused]] uint16_t u16 = 65'535;             // digit separators (C++14)
    [[maybe_unused]] int32_t  i32 = -2'147'483'648;
    [[maybe_unused]] uint32_t u32 = 4'294'967'295u;
    [[maybe_unused]] int64_t  i64 = -9'223'372'036'854'775'807LL;
    [[maybe_unused]] uint64_t u64 = 18'446'744'073'709'551'615ULL;

    // Floating point
    [[maybe_unused]] float       f  = 3.14f;
    [[maybe_unused]] double      d  = 3.141592653589793;
    [[maybe_unused]] long double ld = 3.14159265358979323846L;

    // Boolean & char types
    [[maybe_unused]] bool     b   = true;
    [[maybe_unused]] char     c   = 'A';
    [[maybe_unused]] wchar_t  wc  = L'Z';
    [[maybe_unused]] char8_t  c8  = u8'x';              // C++20
    [[maybe_unused]] char16_t c16 = u'\x6F22';
    [[maybe_unused]] char32_t c32 = U'\x1D11E';

    // Literal types
    [[maybe_unused]] auto bin = 0b1010'1100;             // binary literal (C++14)
    [[maybe_unused]] auto oct = 0775;
    [[maybe_unused]] auto hex = 0xDEAD'BEEF;
    [[maybe_unused]] auto sci = 6.022e23;

    // String literals
    auto s1 = "Hello"s;                                  // std::string literal (C++14)
    auto s2 = "World"sv;                                 // std::string_view literal (C++17)
    auto raw = R"delim(Raw string: no \n escape here)delim"s;

    // nullptr
    [[maybe_unused]] int* ptr = nullptr;

    std::cout << "  Integers: i32=" << i32 << ", u64=" << u64 << '\n';
    std::cout << "  Float: " << f << ", Double: " << d << '\n';
    std::cout << "  String: " << s1 << " " << s2 << '\n';
    std::cout << "  Raw: " << raw << '\n';
    std::cout << "  Binary literal 0b10101100 = " << bin << '\n';

    // std::byte (C++17)
    std::byte byte1{0x42};
    std::byte byte2{0x0F};
    std::cout << "  std::byte XOR: " << std::to_integer<int>(byte1 ^ byte2) << '\n';
}

// ============================================================================
// SECTION 2: auto, decltype, Structured Bindings
// ============================================================================
void showcase_auto_and_bindings() {
    banner("auto, decltype, Structured Bindings");

    // auto deduction
    auto x = 42;
    auto y = 3.14;
    auto z = "hello"s;
    std::cout << "  auto x=" << x << " y=" << y << " z=" << z << '\n';

    // decltype
    decltype(x) a = 100;
    decltype(auto) ref = (a);   // decltype(auto) preserves reference-ness
    ref = 200;
    std::cout << "  decltype: a=" << a << " (modified through ref)\n";

    // Structured bindings (C++17)
    sub("Structured Bindings");
    auto [first, second] = std::pair{42, "answer"s};
    std::cout << "  pair: " << first << ", " << second << '\n';

    std::map<std::string, int> scores{{"Alice", 95}, {"Bob", 87}};
    for (const auto& [name, score] : scores) {
        std::cout << "  " << name << " -> " << score << '\n';
    }

    // Structured binding with tuple
    auto [a1, b1, c1] = std::tuple{1, 2.5, "three"s};
    std::cout << "  tuple: " << a1 << ", " << b1 << ", " << c1 << '\n';

    // Structured binding with array
    int arr[3] = {10, 20, 30};
    auto [p, q, r] = arr;
    std::cout << "  array: " << p << ", " << q << ", " << r << '\n';
}

// ============================================================================
// SECTION 3: Enumerations
// ============================================================================
// Unscoped enum
enum Color { RED, GREEN, BLUE };

// Scoped enum (C++11)
enum class Fruit : uint8_t { Apple, Banana, Cherry };

// Scoped enum with using-enum (C++20)
enum class Direction { North, South, East, West };

std::string_view direction_name(Direction d) {
    using enum Direction;   // C++20: using-enum
    switch (d) {
        case North: return "North";
        case South: return "South";
        case East:  return "East";
        case West:  return "West";
    }
    return "?";
}

void showcase_enumerations() {
    banner("Enumerations");

    Color c = GREEN;
    std::cout << "  Unscoped enum Color::GREEN = " << c << '\n';

    Fruit f = Fruit::Cherry;
    std::cout << "  Scoped enum Fruit::Cherry = " << static_cast<int>(f) << '\n';

    std::cout << "  using-enum Direction: " << direction_name(Direction::North) << '\n';
}

// ============================================================================
// SECTION 4: Control Flow
// ============================================================================
void showcase_control_flow() {
    banner("Control Flow");

    // if-init (C++17)
    sub("if with initializer (C++17)");
    if (auto val = 42; val > 0) {
        std::cout << "  val=" << val << " is positive\n";
    }

    // switch-init (C++17)
    sub("switch with initializer (C++17)");
    switch (auto code = 2; code) {
        case 1: std::cout << "  one\n"; break;
        case 2: std::cout << "  two\n"; break;
        default: std::cout << "  other\n"; break;
    }

    // constexpr-if (C++17)
    sub("constexpr if (C++17)");
    auto check = []<typename T>(T val) {
        if constexpr (std::is_integral_v<T>)
            std::cout << "  integral: " << val << '\n';
        else if constexpr (std::is_floating_point_v<T>)
            std::cout << "  floating: " << val << '\n';
        else
            std::cout << "  other: " << val << '\n';
    };
    check(42);
    check(3.14);
    check("hello");

    // Range-for
    sub("Range-based for loops");
    std::vector v{1, 2, 3, 4, 5};
    for (auto& elem : v) std::cout << "  " << elem;
    std::cout << '\n';

    // Range-for with init (C++20)
    for (auto sz = v.size(); auto& elem : v) {
        std::cout << "  " << elem << "/" << sz;
    }
    std::cout << '\n';
}

// ============================================================================
// SECTION 5: Functions — Overloading, Default Args, Trailing Return
// ============================================================================
// Function overloading
int    add(int a, int b)       { return a + b; }
double add(double a, double b) { return a + b; }

// Default arguments
void greet(std::string_view name, std::string_view greeting = "Hello") {
    std::cout << "  " << greeting << ", " << name << "!\n";
}

// Trailing return type
auto multiply(int a, int b) -> int { return a * b; }

// constexpr function (C++11/14)
constexpr int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

// consteval — immediate function (C++20)
consteval int square(int n) { return n * n; }

// [[nodiscard]] (C++17)
[[nodiscard("Don't ignore the result!")]]
int compute_important_value() { return 42; }

// Variadic function templates (C++11)
template<typename... Args>
auto sum(Args... args) {
    return (args + ...);    // fold expression (C++17)
}

void showcase_functions() {
    banner("Functions");

    std::cout << "  add(1,2)=" << add(1, 2) << '\n';
    std::cout << "  add(1.5,2.5)=" << add(1.5, 2.5) << '\n';
    greet("World");
    greet("World", "Hej");
    std::cout << "  multiply(3,4)=" << multiply(3, 4) << '\n';
    std::cout << "  factorial(10)=" << factorial(10) << '\n';

    constexpr auto sq = square(7);
    std::cout << "  consteval square(7)=" << sq << '\n';

    [[maybe_unused]] auto val = compute_important_value();
    std::cout << "  [[nodiscard]] value=" << val << '\n';

    std::cout << "  fold sum(1,2,3,4,5)=" << sum(1, 2, 3, 4, 5) << '\n';
    std::cout << "  fold sum(1.1,2.2,3.3)=" << sum(1.1, 2.2, 3.3) << '\n';
}

// ============================================================================
// SECTION 6: Lambdas (C++11 -> C++23)
// ============================================================================
void showcase_lambdas() {
    banner("Lambdas");

    // Basic lambda
    auto greet = [](std::string_view name) {
        std::cout << "  Hello, " << name << "!\n";
    };
    greet("Lambda");

    // Capture by value, reference
    int x = 10;
    auto by_val = [x]() { return x * 2; };
    auto by_ref = [&x]() { x += 5; };
    by_ref();
    std::cout << "  x after by_ref: " << x << ", by_val: " << by_val() << '\n';

    // Mutable lambda
    auto counter = [n = 0]() mutable { return ++n; };
    std::cout << "  counter: " << counter() << ", " << counter() << ", " << counter() << '\n';

    // Init-capture (C++14)
    auto msg = "captured"s;
    auto init_cap = [m = std::move(msg)]() { return m; };
    std::cout << "  init-capture: " << init_cap() << '\n';

    // Generic lambda (C++14)
    auto generic = [](auto a, auto b) { return a + b; };
    std::cout << "  generic lambda: " << generic(3, 4) << ", " << generic(1.5, 2.5) << '\n';

    // Template lambda (C++20)
    auto tmpl_lambda = []<typename T>(std::vector<T> const& v) {
        T total{};
        for (auto const& e : v) total += e;
        return total;
    };
    std::cout << "  template lambda sum: " << tmpl_lambda(std::vector{1, 2, 3, 4}) << '\n';

    // Lambda in unevaluated context / constexpr lambda (C++17)
    constexpr auto sq = [](int n) { return n * n; };
    static_assert(sq(5) == 25);
    std::cout << "  constexpr lambda sq(5)=" << sq(5) << '\n';

    // Immediately-invoked lambda expression (IIFE)
    auto result = [](int a, int b) { return a * b; }(6, 7);
    std::cout << "  IIFE: " << result << '\n';

    // Recursive lambda via std::function
    std::function<int(int)> fib = [&fib](int n) -> int {
        return n <= 1 ? n : fib(n - 1) + fib(n - 2);
    };
    std::cout << "  recursive fib(10)=" << fib(10) << '\n';

    // Pack expansion in lambda capture (C++20)
    auto pack_lambda = []<typename... Ts>(Ts... vals) {
        return (... + vals);
    };
    std::cout << "  pack lambda: " << pack_lambda(1, 2, 3, 4, 5) << '\n';
}

// ============================================================================
// SECTION 7: Classes — RAII, Rule of Five, Constructors
// ============================================================================
class ResourceHolder {
    std::string name_;
    int* data_;
    size_t size_;

public:
    // Default constructor
    ResourceHolder() : name_("default"), data_(nullptr), size_(0) {
        std::cout << "    [default ctor: " << name_ << "]\n";
    }

    // Parameterized constructor
    explicit ResourceHolder(std::string name, size_t sz)
        : name_(std::move(name)), data_(new int[sz]), size_(sz) {
        std::fill(data_, data_ + sz, 0);
        std::cout << "    [param ctor: " << name_ << ", size=" << sz << "]\n";
    }

    // Copy constructor
    ResourceHolder(const ResourceHolder& other)
        : name_(other.name_ + "_copy"), data_(new int[other.size_]), size_(other.size_) {
        std::copy(other.data_, other.data_ + size_, data_);
        std::cout << "    [copy ctor: " << name_ << "]\n";
    }

    // Move constructor
    ResourceHolder(ResourceHolder&& other) noexcept
        : name_(std::move(other.name_)), data_(other.data_), size_(other.size_) {
        other.data_ = nullptr;
        other.size_ = 0;
        std::cout << "    [move ctor: " << name_ << "]\n";
    }

    // Copy assignment
    ResourceHolder& operator=(const ResourceHolder& other) {
        if (this != &other) {
            ResourceHolder tmp(other);
            swap(*this, tmp);
            std::cout << "    [copy assign: " << name_ << "]\n";
        }
        return *this;
    }

    // Move assignment
    ResourceHolder& operator=(ResourceHolder&& other) noexcept {
        if (this != &other) {
            delete[] data_;
            name_ = std::move(other.name_);
            data_ = other.data_;
            size_ = other.size_;
            other.data_ = nullptr;
            other.size_ = 0;
            std::cout << "    [move assign: " << name_ << "]\n";
        }
        return *this;
    }

    // Destructor
    ~ResourceHolder() {
        delete[] data_;
    }

    friend void swap(ResourceHolder& a, ResourceHolder& b) noexcept {
        using std::swap;
        swap(a.name_, b.name_);
        swap(a.data_, b.data_);
        swap(a.size_, b.size_);
    }

    std::string_view name() const { return name_; }
    size_t size() const { return size_; }
};

void showcase_classes_raii() {
    banner("Classes: RAII & Rule of Five");

    ResourceHolder r1("alpha", 10);
    ResourceHolder r2 = r1;                  // copy ctor
    ResourceHolder r3 = std::move(r1);       // move ctor
    ResourceHolder r4;
    r4 = r2;                                 // copy assign
    r4 = std::move(r2);                      // move assign

    std::cout << "  r3.name()=" << r3.name() << " size=" << r3.size() << '\n';
}

// ============================================================================
// SECTION 8: Inheritance, Polymorphism, Virtual, Override, Final
// ============================================================================
class Shape {
public:
    virtual ~Shape() = default;
    virtual double area() const = 0;                    // pure virtual
    virtual std::string_view type() const = 0;
    virtual void describe() const {
        std::cout << "  " << type() << " area=" << area() << '\n';
    }
};

class Circle : public Shape {
    double radius_;
public:
    explicit Circle(double r) : radius_(r) {}
    double area() const override { return std::numbers::pi * radius_ * radius_; }
    std::string_view type() const override { return "Circle"; }
};

class Rectangle : public Shape {
protected:
    double w_, h_;
public:
    Rectangle(double w, double h) : w_(w), h_(h) {}
    double area() const override { return w_ * h_; }
    std::string_view type() const override { return "Rectangle"; }
};

class Square final : public Rectangle {
public:
    explicit Square(double s) : Rectangle(s, s) {}
    std::string_view type() const override { return "Square"; }
};

// Multiple inheritance & virtual base
class Printable {
public:
    virtual ~Printable() = default;
    virtual void print(std::ostream& os) const = 0;
};

class PrintableCircle : public Circle, public Printable {
public:
    using Circle::Circle;
    void print(std::ostream& os) const override {
        os << "  PrintableCircle(area=" << area() << ")";
    }
};

void showcase_inheritance() {
    banner("Inheritance & Polymorphism");

    std::vector<std::unique_ptr<Shape>> shapes;
    shapes.push_back(std::make_unique<Circle>(5.0));
    shapes.push_back(std::make_unique<Rectangle>(4.0, 6.0));
    shapes.push_back(std::make_unique<Square>(3.0));

    for (auto& s : shapes) s->describe();

    // RTTI
    sub("RTTI (dynamic_cast, typeid)");
    Shape* sp = shapes[0].get();
    if (auto* cp = dynamic_cast<Circle*>(sp)) {
        std::cout << "  dynamic_cast succeeded: " << cp->type() << '\n';
    }
    std::cout << "  typeid: " << typeid(*sp).name() << '\n';

    // Multiple inheritance
    sub("Multiple Inheritance");
    PrintableCircle pc(7.0);
    pc.print(std::cout);
    std::cout << '\n';
}

// ============================================================================
// SECTION 9: Operator Overloading & Spaceship Operator (C++20)
// ============================================================================
struct Vec2 {
    double x, y;

    Vec2 operator+(Vec2 other) const { return {x + other.x, y + other.y}; }
    Vec2 operator-(Vec2 other) const { return {x - other.x, y - other.y}; }
    Vec2 operator*(double s) const { return {x * s, y * s}; }
    double dot(Vec2 other) const { return x * other.x + y * other.y; }

    // Spaceship operator (C++20) — generates ==, !=, <, <=, >, >=
    auto operator<=>(const Vec2&) const = default;

    friend std::ostream& operator<<(std::ostream& os, Vec2 v) {
        return os << "(" << v.x << "," << v.y << ")";
    }
};

// User-defined literal
constexpr Vec2 operator""_x(long double v) { return {static_cast<double>(v), 0.0}; }
constexpr Vec2 operator""_y(long double v) { return {0.0, static_cast<double>(v)}; }

void showcase_operators() {
    banner("Operator Overloading & <=> (C++20)");

    Vec2 a{1, 2}, b{3, 4};
    std::cout << "  a=" << a << " b=" << b << '\n';
    std::cout << "  a+b=" << (a + b) << '\n';
    std::cout << "  a*3=" << (a * 3) << '\n';
    std::cout << "  a.dot(b)=" << a.dot(b) << '\n';
    std::cout << "  a==a: " << (a == a) << " a<b: " << (a < b) << '\n';

    sub("User-defined Literals");
    auto v = 3.0_x + 4.0_y;
    std::cout << "  3.0_x + 4.0_y = " << v << '\n';
}

// ============================================================================
// SECTION 10: Templates — Basics to Advanced
// ============================================================================
// Function template
template<typename T>
T max_of(T a, T b) { return (a > b) ? a : b; }

// Class template
template<typename T, size_t N>
class StaticVector {
    std::array<T, N> data_{};
    size_t size_ = 0;
public:
    void push_back(const T& val) {
        if (size_ < N) data_[size_++] = val;
    }
    T& operator[](size_t i) { return data_[i]; }
    const T& operator[](size_t i) const { return data_[i]; }
    size_t size() const { return size_; }
    auto begin() const { return data_.begin(); }
    auto end() const { return data_.begin() + size_; }
};

// Variable template (C++14)
template<typename T>
constexpr T pi_v = static_cast<T>(3.14159265358979323846L);

// Template template parameter
template<template<typename, typename> class Container, typename T>
void print_container(const Container<T, std::allocator<T>>& c, std::string_view label) {
    std::cout << "  " << label << ": ";
    for (const auto& e : c) std::cout << e << " ";
    std::cout << '\n';
}

// SFINAE with enable_if (C++11)
template<typename T, std::enable_if_t<std::is_arithmetic_v<T>, int> = 0>
T double_it(T val) { return val * 2; }

// Non-type template parameter (C++20: class types as NTTP)
template<auto Value>
struct Constant {
    static constexpr auto value = Value;
};

// Fold expressions (C++17)
template<typename... Args>
void print_all(Args&&... args) {
    ((std::cout << "  " << args << '\n'), ...);
}

void showcase_templates() {
    banner("Templates");

    std::cout << "  max_of(3,5)=" << max_of(3, 5) << '\n';
    std::cout << "  max_of(3.14,2.72)=" << max_of(3.14, 2.72) << '\n';

    sub("StaticVector (class template)");
    StaticVector<int, 8> sv;
    for (int i : {10, 20, 30}) sv.push_back(i);
    for (auto v : sv) std::cout << "  " << v;
    std::cout << '\n';

    sub("Variable template");
    std::cout << "  pi<float>  = " << pi_v<float> << '\n';
    std::cout << "  pi<double> = " << pi_v<double> << '\n';

    sub("Template template parameter");
    print_container(std::vector{1, 2, 3}, "vector");
    print_container(std::deque{4, 5, 6}, "deque");

    sub("SFINAE");
    std::cout << "  double_it(21) = " << double_it(21) << '\n';
    std::cout << "  double_it(1.5) = " << double_it(1.5) << '\n';

    sub("Non-type template parameter");
    std::cout << "  Constant<42>::value = " << Constant<42>::value << '\n';
    std::cout << "  Constant<'A'>::value = " << Constant<'A'>::value << '\n';

    sub("Fold expressions / print_all");
    print_all(1, 2.5, "three", 'D');
}

// ============================================================================
// SECTION 11: Concepts & Constraints (C++20)
// ============================================================================
// Define a concept
template<typename T>
concept Arithmetic = std::is_arithmetic_v<T>;

template<typename T>
concept Addable = requires(T a, T b) {
    { a + b } -> std::convertible_to<T>;
};

template<typename T>
concept Printable_ = requires(std::ostream& os, T val) {
    { os << val } -> std::same_as<std::ostream&>;
};

template<typename T>
concept Container = requires(T c) {
    typename T::value_type;
    { c.begin() } -> std::input_or_output_iterator;
    { c.end() }   -> std::input_or_output_iterator;
    { c.size() }  -> std::convertible_to<std::size_t>;
};

// Constrained function (shorthand)
void print_arithmetic(Arithmetic auto val) {
    std::cout << "  arithmetic: " << val << '\n';
}

// Constrained function (requires clause)
template<typename T>
    requires Addable<T> && Printable_<T>
T constrained_add(T a, T b) {
    auto result = a + b;
    std::cout << "  constrained_add: " << a << " + " << b << " = " << result << '\n';
    return result;
}

// Concept with Container
template<Container C>
void describe_container(const C& c) {
    std::cout << "  container size=" << c.size()
              << " typeid=" << typeid(typename C::value_type).name() << '\n';
}

void showcase_concepts() {
    banner("Concepts & Constraints (C++20)");

    print_arithmetic(42);
    print_arithmetic(3.14);

    constrained_add(10, 20);
    constrained_add(1.5, 2.5);
    constrained_add("hello "s, "world"s);

    describe_container(std::vector{1, 2, 3});
    describe_container(std::list{4.0, 5.0, 6.0});
}

// ============================================================================
// SECTION 12: Standard Containers
// ============================================================================
void showcase_containers() {
    banner("Standard Containers");

    sub("vector");
    std::vector<int> vec{1, 2, 3, 4, 5};
    vec.push_back(6);
    vec.emplace_back(7);
    std::cout << "  vec: ";
    for (auto v : vec) std::cout << v << " ";
    std::cout << "(cap=" << vec.capacity() << ")\n";

    sub("array");
    std::array<int, 5> arr{10, 20, 30, 40, 50};
    std::cout << "  array[2]=" << arr[2] << " size=" << arr.size() << '\n';

    sub("map & unordered_map");
    std::map<std::string, int> ordered{{"Alice", 90}, {"Bob", 85}, {"Carol", 92}};
    for (auto& [k, v] : ordered) std::cout << "  " << k << ":" << v << " ";
    std::cout << '\n';

    std::unordered_map<std::string, int> unord{{"x", 1}, {"y", 2}};
    unord.try_emplace("z", 3);      // C++17
    unord.insert_or_assign("x", 10);// C++17
    std::cout << "  unordered_map: x=" << unord["x"] << " z=" << unord["z"] << '\n';

    sub("set & multiset");
    std::set<int> s{3, 1, 4, 1, 5, 9};
    std::cout << "  set: ";
    for (auto v : s) std::cout << v << " ";
    std::cout << "(contains 4: " << s.contains(4) << ")\n";   // C++20

    sub("deque, list, queue, stack, priority_queue");
    std::deque<int> dq{1, 2, 3};
    dq.push_front(0);
    dq.push_back(4);

    std::priority_queue<int> pq;
    for (int v : {3, 1, 4, 1, 5}) pq.push(v);
    std::cout << "  priority_queue top: " << pq.top() << '\n';

    sub("span (C++20)");
    int raw[] = {10, 20, 30, 40, 50};
    std::span<int> sp(raw);
    std::cout << "  span: ";
    for (auto v : sp.subspan(1, 3)) std::cout << v << " ";
    std::cout << '\n';
}

// ============================================================================
// SECTION 13: Iterators & Algorithms
// ============================================================================
void showcase_algorithms() {
    banner("Iterators & Algorithms");

    std::vector<int> data{5, 3, 8, 1, 9, 2, 7, 4, 6};

    sub("sort, reverse, find");
    std::sort(data.begin(), data.end());
    std::cout << "  sorted: ";
    for (auto v : data) std::cout << v << " ";
    std::cout << '\n';

    std::reverse(data.begin(), data.end());
    std::cout << "  reversed: ";
    for (auto v : data) std::cout << v << " ";
    std::cout << '\n';

    auto it = std::find(data.begin(), data.end(), 7);
    std::cout << "  find(7): position " << std::distance(data.begin(), it) << '\n';

    sub("transform, accumulate, reduce");
    std::vector<int> doubled(data.size());
    std::transform(data.begin(), data.end(), doubled.begin(), [](int n) { return n * 2; });
    std::cout << "  doubled: ";
    for (auto v : doubled) std::cout << v << " ";
    std::cout << '\n';

    auto total = std::accumulate(data.begin(), data.end(), 0);
    std::cout << "  accumulate: " << total << '\n';

    auto total2 = std::reduce(data.begin(), data.end(), 0);  // C++17
    std::cout << "  reduce: " << total2 << '\n';

    sub("partition, nth_element, partial_sort");
    std::vector v2{5, 3, 8, 1, 9, 2, 7};
    auto mid = std::partition(v2.begin(), v2.end(), [](int n) { return n % 2 == 0; });
    std::cout << "  partitioned (evens first): ";
    for (auto v : v2) std::cout << v << " ";
    std::cout << "(pivot at " << std::distance(v2.begin(), mid) << ")\n";

    sub("any_of, all_of, none_of, count_if");
    std::vector v3{2, 4, 6, 8, 10};
    std::cout << "  all even: " << std::all_of(v3.begin(), v3.end(), [](int n) { return n % 2 == 0; }) << '\n';
    std::cout << "  any > 5: " << std::any_of(v3.begin(), v3.end(), [](int n) { return n > 5; }) << '\n';

    sub("min_element, max_element, minmax_element");
    auto [mn, mx] = std::minmax_element(data.begin(), data.end());
    std::cout << "  min=" << *mn << " max=" << *mx << '\n';

    sub("iota, generate");
    std::vector<int> seq(10);
    std::iota(seq.begin(), seq.end(), 1);
    std::cout << "  iota: ";
    for (auto v : seq) std::cout << v << " ";
    std::cout << '\n';
}

// ============================================================================
// SECTION 14: Ranges & Views (C++20)
// ============================================================================
void showcase_ranges() {
    banner("Ranges & Views (C++20)");

    namespace rv = std::views;

    std::vector<int> data{1, 2, 3, 4, 5, 6, 7, 8, 9, 10};

    sub("filter | transform | take");
    auto pipeline = data
        | rv::filter([](int n) { return n % 2 == 0; })
        | rv::transform([](int n) { return n * n; })
        | rv::take(3);
    std::cout << "  even squares (take 3): ";
    for (auto v : pipeline) std::cout << v << " ";
    std::cout << '\n';

    sub("reverse | drop");
    auto rev = data | rv::reverse | rv::drop(3);
    std::cout << "  reverse|drop(3): ";
    for (auto v : rev) std::cout << v << " ";
    std::cout << '\n';

    sub("iota | transform");
    auto squares = rv::iota(1, 11) | rv::transform([](int n) { return n * n; });
    std::cout << "  squares 1-10: ";
    for (auto v : squares) std::cout << v << " ";
    std::cout << '\n';

    sub("zip (C++23)");
#if __cpp_lib_ranges_zip >= 202110L
    std::vector<std::string> names{"Alice", "Bob", "Carol"};
    std::vector<int> scores{95, 87, 92};
    for (auto [name, score] : rv::zip(names, scores)) {
        std::cout << "  " << name << ": " << score << '\n';
    }
#else
    std::cout << "  (zip not available -- requires C++23 library support)\n";
#endif

    sub("enumerate (C++23)");
#if __cpp_lib_ranges_enumerate >= 202302L
    for (auto [i, v] : data | rv::enumerate | rv::take(5)) {
        std::cout << "  [" << i << "]=" << v << " ";
    }
    std::cout << '\n';
#else
    std::cout << "  (enumerate not available -- requires C++23 library support)\n";
#endif

    sub("chunk (C++23)");
#if __cpp_lib_ranges_chunk >= 202202L
    std::cout << "  chunks of 3: ";
    for (auto chunk : data | rv::chunk(3)) {
        std::cout << "{ ";
        for (auto v : chunk) std::cout << v << " ";
        std::cout << "} ";
    }
    std::cout << '\n';
#else
    std::cout << "  (chunk not available -- requires C++23 library support)\n";
#endif
}

// ============================================================================
// SECTION 15: Smart Pointers
// ============================================================================
struct Node {
    int value;
    std::shared_ptr<Node> next;
    Node(int v) : value(v) { std::cout << "    Node(" << v << ") created\n"; }
    ~Node() { std::cout << "    Node(" << value << ") destroyed\n"; }
};

void showcase_smart_pointers() {
    banner("Smart Pointers");

    sub("unique_ptr");
    {
        auto up1 = std::make_unique<Node>(1);
        std::cout << "  up1->value = " << up1->value << '\n';

        auto up2 = std::move(up1);  // ownership transfer
        std::cout << "  up1 is " << (up1 ? "valid" : "null") << '\n';
        std::cout << "  up2->value = " << up2->value << '\n';
    }

    sub("shared_ptr");
    {
        auto sp1 = std::make_shared<Node>(2);
        {
            auto sp2 = sp1;
            std::cout << "  refcount = " << sp1.use_count() << '\n';
        }
        std::cout << "  refcount after scope = " << sp1.use_count() << '\n';
    }

    sub("weak_ptr");
    {
        std::weak_ptr<Node> wp;
        {
            auto sp = std::make_shared<Node>(3);
            wp = sp;
            if (auto locked = wp.lock()) {
                std::cout << "  weak_ptr locked: " << locked->value << '\n';
            }
        }
        std::cout << "  weak_ptr expired: " << wp.expired() << '\n';
    }

    sub("unique_ptr with custom deleter");
    {
        auto deleter = [](int* p) {
            std::cout << "    custom deleter called for " << *p << '\n';
            delete p;
        };
        std::unique_ptr<int, decltype(deleter)> cp(new int(99), deleter);
        std::cout << "  custom deleter ptr: " << *cp << '\n';
    }
}

// ============================================================================
// SECTION 16: optional, variant, any, expected
// ============================================================================
std::optional<int> safe_divide(int a, int b) {
    if (b == 0) return std::nullopt;
    return a / b;
}

using JsonValue = std::variant<std::monostate, int, double, std::string, bool>;

void showcase_vocabulary_types() {
    banner("optional, variant, any");

    sub("std::optional");
    auto r1 = safe_divide(10, 3);
    auto r2 = safe_divide(10, 0);
    std::cout << "  10/3 = " << r1.value_or(-1) << '\n';
    std::cout << "  10/0 = " << r2.value_or(-1) << " (nullopt)\n";

    // optional monadic ops (C++23)
#if __cpp_lib_optional >= 202110L
    auto result = safe_divide(100, 5)
        .transform([](int v) { return v * 2; })
        .and_then([](int v) -> std::optional<int> {
            return v > 10 ? std::optional(v) : std::nullopt;
        });
    std::cout << "  monadic: " << result.value_or(0) << '\n';
#else
    std::cout << "  (monadic optional ops require C++23)\n";
#endif

    sub("std::variant");
    JsonValue jv = 42;
    std::cout << "  variant holds int: " << std::get<int>(jv) << '\n';

    jv = "hello"s;
    std::cout << "  variant holds string: " << std::get<std::string>(jv) << '\n';

    // std::visit
    auto visitor = [](auto&& val) {
        using T = std::decay_t<decltype(val)>;
        if constexpr (std::is_same_v<T, std::monostate>)
            std::cout << "  visit: monostate\n";
        else
            std::cout << "  visit: " << val << '\n';
    };
    std::visit(visitor, jv);

    std::cout << "  index=" << jv.index() << '\n';
    std::cout << "  holds string: " << std::holds_alternative<std::string>(jv) << '\n';

    sub("std::any");
    std::any a = 42;
    std::cout << "  any<int>: " << std::any_cast<int>(a) << '\n';
    a = "hello"s;
    std::cout << "  any<string>: " << std::any_cast<std::string>(a) << '\n';
    std::cout << "  any type: " << a.type().name() << '\n';

    try {
        [[maybe_unused]] auto bad = std::any_cast<double>(a);
    } catch (const std::bad_any_cast& e) {
        std::cout << "  bad_any_cast: " << e.what() << '\n';
    }
}

// ============================================================================
// SECTION 17: Tuples & Pairs
// ============================================================================
void showcase_tuples() {
    banner("Tuples & Pairs");

    auto t = std::make_tuple(1, 2.5, "hello"s, true);
    std::cout << "  tuple: " << std::get<0>(t) << ", " << std::get<1>(t)
              << ", " << std::get<2>(t) << ", " << std::get<3>(t) << '\n';

    // std::apply
    auto sum_tup = std::apply([](auto... args) { return (... + args); },
                              std::tuple{1, 2, 3, 4});
    std::cout << "  std::apply sum: " << sum_tup << '\n';

    // std::tie for comparison
    auto person1 = std::tuple{"Alice"sv, 30};
    auto person2 = std::tuple{"Bob"sv, 25};
    std::cout << "  tuple comparison (Alice,30)<(Bob,25): " << (person1 < person2) << '\n';

    // tuple_cat
    auto combined = std::tuple_cat(std::tuple{1, 2}, std::tuple{3.0, "four"s});
    std::cout << "  tuple_cat size: " << std::tuple_size_v<decltype(combined)> << '\n';
}

// ============================================================================
// SECTION 18: Type Traits & SFINAE
// ============================================================================
void showcase_type_traits() {
    banner("Type Traits & Compile-time Reflection");

    std::cout << "  is_integral<int>: " << std::is_integral_v<int> << '\n';
    std::cout << "  is_floating<double>: " << std::is_floating_point_v<double> << '\n';
    std::cout << "  is_same<int,int>: " << std::is_same_v<int, int> << '\n';
    std::cout << "  is_same<int,long>: " << std::is_same_v<int, long> << '\n';
    std::cout << "  is_base_of<Shape,Circle>: " << std::is_base_of_v<Shape, Circle> << '\n';
    std::cout << "  is_polymorphic<Shape>: " << std::is_polymorphic_v<Shape> << '\n';
    std::cout << "  is_abstract<Shape>: " << std::is_abstract_v<Shape> << '\n';
    std::cout << "  is_trivially_copyable<int>: " << std::is_trivially_copyable_v<int> << '\n';
    std::cout << "  is_nothrow_move_constructible<string>: "
              << std::is_nothrow_move_constructible_v<std::string> << '\n';

    sub("conditional, decay, remove_reference");
    using T1 = std::conditional_t<true, int, double>;
    using T2 = std::decay_t<const int&>;
    using T3 = std::remove_reference_t<int&&>;
    std::cout << "  conditional<true,int,double> is int: " << std::is_same_v<T1, int> << '\n';
    std::cout << "  decay<const int&> is int: " << std::is_same_v<T2, int> << '\n';
    std::cout << "  remove_reference<int&&> is int: " << std::is_same_v<T3, int> << '\n';
}

// ============================================================================
// SECTION 19: Compile-time Computation
// ============================================================================
// constexpr class
class ConstexprVec {
    double x_, y_;
public:
    constexpr ConstexprVec(double x, double y) : x_(x), y_(y) {}
    constexpr double magnitude_sq() const { return x_ * x_ + y_ * y_; }
    constexpr ConstexprVec operator+(ConstexprVec o) const { return {x_ + o.x_, y_ + o.y_}; }
};

// static_assert
static_assert(factorial(5) == 120, "factorial check");
static_assert(ConstexprVec(3, 4).magnitude_sq() == 25.0, "magnitude check");

// if constexpr dispatch
template<typename T>
std::string type_name_of() {
    if constexpr (std::is_integral_v<T>) return "integer";
    else if constexpr (std::is_floating_point_v<T>) return "float";
    else if constexpr (std::is_same_v<T, std::string>) return "string";
    else return "unknown";
}

void showcase_constexpr() {
    banner("Compile-time Computation");

    constexpr auto f10 = factorial(10);
    std::cout << "  constexpr factorial(10) = " << f10 << '\n';

    constexpr ConstexprVec v1(3, 4), v2(1, 2);
    constexpr auto v3 = v1 + v2;
    constexpr auto mag = v3.magnitude_sq();
    std::cout << "  constexpr vec: mag_sq=" << mag << '\n';

    std::cout << "  type_name_of<int>: " << type_name_of<int>() << '\n';
    std::cout << "  type_name_of<double>: " << type_name_of<double>() << '\n';
    std::cout << "  type_name_of<std::string>: " << type_name_of<std::string>() << '\n';

    sub("std::source_location (C++20)");
    auto loc = std::source_location::current();
    std::cout << "  file: " << loc.file_name() << '\n';
    std::cout << "  line: " << loc.line() << " col: " << loc.column() << '\n';
    std::cout << "  function: " << loc.function_name() << '\n';
}

// ============================================================================
// SECTION 20: Error Handling — Exceptions
// ============================================================================
class AppError : public std::runtime_error {
    int code_;
public:
    AppError(int code, const std::string& msg)
        : std::runtime_error(msg), code_(code) {}
    int code() const noexcept { return code_; }
};

// noexcept specification
int safe_add(int a, int b) noexcept {
    return a + b;
}

void showcase_exceptions() {
    banner("Error Handling -- Exceptions");

    sub("try / catch / throw");
    try {
        throw AppError(404, "Resource not found");
    } catch (const AppError& e) {
        std::cout << "  AppError code=" << e.code() << " msg=" << e.what() << '\n';
    }

    // Nested exceptions (C++11)
    sub("Nested exceptions");
    try {
        throw std::runtime_error("simulated error");
    } catch (const std::runtime_error& e) {
        std::cout << "  caught: " << e.what() << '\n';
    }

    sub("noexcept");
    std::cout << "  safe_add is noexcept: " << noexcept(safe_add(1, 2)) << '\n';
    std::cout << "  safe_add(1,2)=" << safe_add(1, 2) << '\n';
}

// ============================================================================
// SECTION 21: Move Semantics & Perfect Forwarding
// ============================================================================
struct HeavyObject {
    std::string data;
    HeavyObject(std::string s) : data(std::move(s)) {
        std::cout << "    HeavyObject constructed: " << data << '\n';
    }
    HeavyObject(const HeavyObject& o) : data(o.data) {
        std::cout << "    HeavyObject copied: " << data << '\n';
    }
    HeavyObject(HeavyObject&& o) noexcept : data(std::move(o.data)) {
        std::cout << "    HeavyObject moved\n";
    }
};

// Perfect forwarding factory
template<typename T, typename... Args>
std::unique_ptr<T> make(Args&&... args) {
    return std::make_unique<T>(std::forward<Args>(args)...);
}

void showcase_move_semantics() {
    banner("Move Semantics & Perfect Forwarding");

    sub("std::move");
    HeavyObject h1("hello");
    HeavyObject h2 = std::move(h1);
    std::cout << "  h1.data after move: '" << h1.data << "'\n";

    sub("Perfect forwarding");
    auto ptr = make<HeavyObject>("forwarded");
    std::cout << "  forwarded result: " << ptr->data << '\n';

    sub("Reference collapsing");
    int x = 42;
    auto&& r1 = x;           // int&  (lvalue -> lvalue ref)
    auto&& r2 = std::move(x);// int&& (rvalue -> rvalue ref)
    std::cout << "  r1 (lvalue ref): " << r1 << '\n';
    std::cout << "  r2 (rvalue ref): " << r2 << '\n';
}

// ============================================================================
// SECTION 22: std::function, std::bind, Callables
// ============================================================================
int multiply_fn(int a, int b) { return a * b; }

struct Functor {
    int factor;
    int operator()(int x) const { return x * factor; }
};

void showcase_callables() {
    banner("std::function, std::bind, Callables");

    // std::function wrapping different callables
    std::function<int(int, int)> fn;

    fn = multiply_fn;
    std::cout << "  function ptr: " << fn(3, 4) << '\n';

    fn = [](int a, int b) { return a + b; };
    std::cout << "  lambda: " << fn(3, 4) << '\n';

    Functor f{10};
    std::function<int(int)> fn2 = f;
    std::cout << "  functor: " << fn2(5) << '\n';

    // std::bind
    auto bound = std::bind(multiply_fn, std::placeholders::_1, 10);
    std::cout << "  bind(*,10)(7): " << bound(7) << '\n';

    // std::invoke (C++17)
    std::cout << "  std::invoke: " << std::invoke(multiply_fn, 6, 7) << '\n';

    struct Obj {
        int value = 42;
        int get() const { return value; }
    };
    Obj obj;
    std::cout << "  invoke member fn: " << std::invoke(&Obj::get, obj) << '\n';
    std::cout << "  invoke member ptr: " << std::invoke(&Obj::value, obj) << '\n';
}

// ============================================================================
// SECTION 23: Strings, String Views, Formatting
// ============================================================================
void showcase_strings() {
    banner("Strings, String Views, Formatting");

    std::string s = "Hello, World!";
    std::string_view sv = s;

    sub("std::string operations");
    std::cout << "  substr: " << s.substr(0, 5) << '\n';
    std::cout << "  find: " << s.find("World") << '\n';
    std::cout << "  starts_with: " << s.starts_with("Hello") << '\n';    // C++20
    std::cout << "  ends_with: " << s.ends_with("!") << '\n';            // C++20
    std::cout << "  contains: " << s.contains("World") << '\n';          // C++23

    sub("std::string_view (zero-copy)");
    std::cout << "  sv: " << sv << " size=" << sv.size() << '\n';
    std::cout << "  sv.substr(7,5): " << sv.substr(7, 5) << '\n';

    sub("std::format (C++20)");
    std::cout << std::format("  formatted: {} + {} = {}\n", 1, 2, 3);
    std::cout << std::format("  padded: [{:>10}]\n", "right");
    std::cout << std::format("  padded: [{:<10}]\n", "left");
    std::cout << std::format("  padded: [{:^10}]\n", "center");
    std::cout << std::format("  hex: {:#x}\n", 255);
    std::cout << std::format("  binary: {:#b}\n", 42);
    std::cout << std::format("  float: {:.4f}\n", 3.14159);
    std::cout << std::format("  sci: {:.2e}\n", 123456.789);

    sub("String conversion");
    std::cout << "  stoi: " << std::stoi("42") << '\n';
    std::cout << "  stod: " << std::stod("3.14") << '\n';
    std::cout << "  to_string: " << std::to_string(42) << '\n';
}

// ============================================================================
// SECTION 24: Regular Expressions
// ============================================================================
void showcase_regex() {
    banner("Regular Expressions");

    std::string text = "Contact: alice@example.com or bob@test.org";
    std::regex email_re(R"((\w+)@(\w+\.\w+))");

    sub("regex_search");
    std::smatch match;
    if (std::regex_search(text, match, email_re)) {
        std::cout << "  full match: " << match[0] << '\n';
        std::cout << "  user: " << match[1] << " domain: " << match[2] << '\n';
    }

    sub("regex_iterator (find all)");
    auto begin = std::sregex_iterator(text.begin(), text.end(), email_re);
    auto end = std::sregex_iterator();
    for (auto it = begin; it != end; ++it) {
        std::cout << "  found: " << (*it)[0] << '\n';
    }

    sub("regex_replace");
    auto redacted = std::regex_replace(text, email_re, "[REDACTED]");
    std::cout << "  " << redacted << '\n';
}

// ============================================================================
// SECTION 25: Concurrency — Threads, Mutex, Atomics
// ============================================================================
void showcase_concurrency() {
    banner("Concurrency");

    sub("std::thread & std::mutex");
    std::mutex mtx;
    int shared_counter = 0;
    auto worker = [&]([[maybe_unused]] int id, int iterations) {
        for (int i = 0; i < iterations; ++i) {
            std::lock_guard lock(mtx);  // CTAD (C++17)
            ++shared_counter;
        }
    };

    {
        std::vector<std::jthread> threads;   // C++20: auto-joining
        for (int i = 0; i < 4; ++i)
            threads.emplace_back(worker, i, 1000);
    }
    std::cout << "  counter (4 threads x 1000): " << shared_counter << '\n';

    sub("std::atomic");
    std::atomic<int> atomic_counter{0};
    {
        std::vector<std::jthread> threads;
        for (int i = 0; i < 4; ++i)
            threads.emplace_back([&] {
                for (int j = 0; j < 1000; ++j)
                    atomic_counter.fetch_add(1, std::memory_order_relaxed);
            });
    }
    std::cout << "  atomic counter: " << atomic_counter.load() << '\n';

    sub("std::shared_mutex (reader-writer lock)");
    std::shared_mutex rw_mutex;
    int data = 0;

    auto reader = [&]([[maybe_unused]] int id) {
        std::shared_lock lock(rw_mutex);
        [[maybe_unused]] auto val = data;
    };
    auto writer_fn = [&](int val) {
        std::unique_lock lock(rw_mutex);
        data = val;
    };

    {
        std::jthread r1(reader, 1), r2(reader, 2);
        std::jthread w1(writer_fn, 42);
    }
    std::cout << "  shared data after write: " << data << '\n';

    sub("std::async & std::future");
    auto fut = std::async(std::launch::async, [] {
        std::this_thread::sleep_for(10ms);
        return 42;
    });
    std::cout << "  future result: " << fut.get() << '\n';

    sub("std::promise");
    std::promise<int> promise;
    auto pf = promise.get_future();
    std::jthread([&] { promise.set_value(99); });
    std::cout << "  promise result: " << pf.get() << '\n';

    sub("std::counting_semaphore (C++20)");
    std::counting_semaphore<2> sem(2);
    std::atomic<int> sem_count{0};
    {
        std::vector<std::jthread> threads;
        for (int i = 0; i < 5; ++i) {
            threads.emplace_back([&] {
                sem.acquire();
                sem_count.fetch_add(1);
                std::this_thread::sleep_for(1ms);
                sem.release();
            });
        }
    }
    std::cout << "  semaphore tasks completed: " << sem_count.load() << '\n';

    sub("std::latch (C++20)");
    std::latch latch(3);
    std::atomic<int> latch_done{0};
    auto latch_worker = [&] {
        latch_done.fetch_add(1);
        latch.count_down();
    };
    {
        std::jthread t1(latch_worker), t2(latch_worker), t3(latch_worker);
    }
    latch.wait();
    std::cout << "  latch completed: " << latch_done.load() << " workers\n";

    sub("std::barrier (C++20)");
    int phase = 0;
    std::barrier barrier(3, [&]() noexcept { ++phase; });
    {
        std::vector<std::jthread> threads;
        for (int i = 0; i < 3; ++i) {
            threads.emplace_back([&] {
                barrier.arrive_and_wait();
                barrier.arrive_and_wait();
            });
        }
    }
    std::cout << "  barrier phases completed: " << phase << '\n';

    sub("std::condition_variable");
    std::mutex cv_mtx;
    std::condition_variable cv;
    bool ready = false;
    int cv_data = 0;

    std::jthread producer([&] {
        {
            std::lock_guard lock(cv_mtx);
            cv_data = 77;
            ready = true;
        }
        cv.notify_one();
    });

    {
        std::unique_lock lock(cv_mtx);
        cv.wait(lock, [&] { return ready; });
        std::cout << "  condition_variable data: " << cv_data << '\n';
    }
}

// ============================================================================
// SECTION 26: Coroutines (C++20) — Lazy Generator
// ============================================================================
template<typename T>
struct Generator {
    struct promise_type {
        T current_value;

        Generator get_return_object() {
            return Generator{std::coroutine_handle<promise_type>::from_promise(*this)};
        }
        std::suspend_always initial_suspend() noexcept { return {}; }
        std::suspend_always final_suspend() noexcept { return {}; }
        std::suspend_always yield_value(T value) noexcept {
            current_value = std::move(value);
            return {};
        }
        void return_void() noexcept {}
        void unhandled_exception() { std::terminate(); }
    };

    using handle_type = std::coroutine_handle<promise_type>;
    handle_type handle_;

    explicit Generator(handle_type h) : handle_(h) {}
    ~Generator() { if (handle_) handle_.destroy(); }

    Generator(const Generator&) = delete;
    Generator(Generator&& o) noexcept : handle_(o.handle_) { o.handle_ = nullptr; }

    bool next() {
        handle_.resume();
        return !handle_.done();
    }
    T value() const { return handle_.promise().current_value; }

    // Range support
    struct iterator {
        handle_type handle;
        bool done;

        iterator& operator++() {
            handle.resume();
            done = handle.done();
            return *this;
        }
        T operator*() const { return handle.promise().current_value; }
        bool operator!=(std::default_sentinel_t) const { return !done; }
    };

    iterator begin() {
        handle_.resume();
        return {handle_, handle_.done()};
    }
    std::default_sentinel_t end() { return {}; }
};

Generator<int> fibonacci(int limit) {
    int a = 0, b = 1;
    for (int i = 0; i < limit; ++i) {
        co_yield a;
        auto next = a + b;
        a = b;
        b = next;
    }
}

Generator<int> range_gen(int start, int end_val, int step = 1) {
    for (int i = start; i < end_val; i += step)
        co_yield i;
}

void showcase_coroutines() {
    banner("Coroutines (C++20)");

    sub("Fibonacci generator");
    std::cout << "  fib: ";
    for (auto v : fibonacci(12))
        std::cout << v << " ";
    std::cout << '\n';

    sub("Range generator");
    std::cout << "  range(0,20,3): ";
    for (auto v : range_gen(0, 20, 3))
        std::cout << v << " ";
    std::cout << '\n';
}

// ============================================================================
// SECTION 27: Chrono & Time
// ============================================================================
void showcase_chrono() {
    banner("Chrono & Time");

    sub("Duration arithmetic");
    auto duration = 2h + 30min + 15s + 500ms;
    auto total_ms = std::chrono::duration_cast<std::chrono::milliseconds>(duration);
    std::cout << "  2h30m15s500ms = " << total_ms.count() << " ms\n";

    sub("Timing execution");
    auto start = std::chrono::high_resolution_clock::now();
    volatile long long sum_val = 0;
    for (int i = 0; i < 1'000'000; ++i) sum_val += i;
    auto end = std::chrono::high_resolution_clock::now();
    auto elapsed = std::chrono::duration_cast<std::chrono::microseconds>(end - start);
    std::cout << "  1M additions: " << elapsed.count() << " us\n";

    sub("System clock");
    auto now = std::chrono::system_clock::now();
    auto epoch = now.time_since_epoch();
    auto secs = std::chrono::duration_cast<std::chrono::seconds>(epoch);
    std::cout << "  seconds since epoch: " << secs.count() << '\n';
}

// ============================================================================
// SECTION 28: Random Numbers
// ============================================================================
void showcase_random() {
    banner("Random Numbers");

    std::random_device rd;
    std::mt19937 gen(rd());

    sub("Uniform distributions");
    std::uniform_int_distribution<int> int_dist(1, 100);
    std::uniform_real_distribution<double> real_dist(0.0, 1.0);
    std::cout << "  int [1,100]: ";
    for (int i = 0; i < 5; ++i) std::cout << int_dist(gen) << " ";
    std::cout << "\n  real [0,1): ";
    for (int i = 0; i < 5; ++i) std::cout << std::format("{:.3f} ", real_dist(gen));
    std::cout << '\n';

    sub("Normal distribution");
    std::normal_distribution<double> norm(0.0, 1.0);
    std::cout << "  normal(0,1): ";
    for (int i = 0; i < 5; ++i) std::cout << std::format("{:.3f} ", norm(gen));
    std::cout << '\n';

    sub("Shuffle");
    std::vector<int> deck{1, 2, 3, 4, 5, 6, 7, 8, 9, 10};
    std::shuffle(deck.begin(), deck.end(), gen);
    std::cout << "  shuffled: ";
    for (auto v : deck) std::cout << v << " ";
    std::cout << '\n';
}

// ============================================================================
// SECTION 29: Filesystem (C++17)
// ============================================================================
void showcase_filesystem() {
    banner("Filesystem (C++17)");

    auto tmp = fs::temp_directory_path() / "cpp_showcase_test";
    fs::create_directories(tmp / "subdir");

    // Write a file
    {
        std::ofstream(tmp / "test.txt") << "Hello, filesystem!";
    }

    std::cout << "  temp path: " << tmp << '\n';
    std::cout << "  exists: " << fs::exists(tmp) << '\n';
    std::cout << "  is_directory: " << fs::is_directory(tmp) << '\n';

    sub("Directory iteration");
    for (auto& entry : fs::recursive_directory_iterator(tmp)) {
        std::cout << "  " << (entry.is_directory() ? "[D] " : "[F] ")
                  << entry.path().filename() << '\n';
    }

    sub("File info");
    auto file_path = tmp / "test.txt";
    std::cout << "  size: " << fs::file_size(file_path) << " bytes\n";
    std::cout << "  extension: " << file_path.extension() << '\n';
    std::cout << "  stem: " << file_path.stem() << '\n';
    std::cout << "  parent: " << file_path.parent_path() << '\n';

    // Cleanup
    fs::remove_all(tmp);
    std::cout << "  cleaned up temp directory\n";
}

// ============================================================================
// SECTION 30: Bit Manipulation (C++20)
// ============================================================================
void showcase_bits() {
    banner("Bit Manipulation (C++20)");

    uint32_t val = 0b0000'0000'0000'0000'0000'0000'0010'1010;  // 42

    std::cout << "  value: " << val << " (0b" << std::bitset<8>(val) << ")\n";
    std::cout << "  popcount: " << std::popcount(val) << '\n';
    std::cout << "  has_single_bit: " << std::has_single_bit(val) << '\n';
    std::cout << "  bit_ceil: " << std::bit_ceil(val) << '\n';
    std::cout << "  bit_floor: " << std::bit_floor(val) << '\n';
    std::cout << "  bit_width: " << std::bit_width(val) << '\n';
    std::cout << "  countl_zero: " << std::countl_zero(val) << '\n';
    std::cout << "  countr_zero: " << std::countr_zero(val) << '\n';
    std::cout << "  rotl(val,2): " << std::bitset<8>(std::rotl(static_cast<uint8_t>(val), 2)) << '\n';
    std::cout << "  rotr(val,2): " << std::bitset<8>(std::rotr(static_cast<uint8_t>(val), 2)) << '\n';

    sub("std::bitset");
    std::bitset<16> bits("1010110011001010");
    std::cout << "  bitset: " << bits << '\n';
    std::cout << "  count: " << bits.count() << '\n';
    std::cout << "  flip: " << bits.flip() << '\n';
    std::cout << "  test(0): " << bits.test(0) << '\n';
}

// ============================================================================
// SECTION 31: Numeric Constants & Math (C++20)
// ============================================================================
void showcase_numbers() {
    banner("Numeric Constants & Math (C++20)");

    std::cout << "  std::numbers::pi     = " << std::numbers::pi << '\n';
    std::cout << "  std::numbers::e      = " << std::numbers::e << '\n';
    std::cout << "  std::numbers::sqrt2  = " << std::numbers::sqrt2 << '\n';
    std::cout << "  std::numbers::phi    = " << std::numbers::phi << '\n';
    std::cout << "  std::numbers::ln2    = " << std::numbers::ln2 << '\n';
    std::cout << "  std::numbers::ln10   = " << std::numbers::ln10 << '\n';
    std::cout << "  std::numbers::log2e  = " << std::numbers::log2e << '\n';
    std::cout << "  std::numbers::log10e = " << std::numbers::log10e << '\n';

    sub("Math operations");
    std::cout << "  lerp(0,10,0.3): " << std::lerp(0.0, 10.0, 0.3) << '\n';
    std::cout << "  midpoint(3,7): " << std::midpoint(3, 7) << '\n';
    std::cout << "  gcd(12,8): " << std::gcd(12, 8) << '\n';
    std::cout << "  lcm(12,8): " << std::lcm(12, 8) << '\n';
    std::cout << "  clamp(15,0,10): " << std::clamp(15, 0, 10) << '\n';
}

// ============================================================================
// SECTION 32: Attributes
// ============================================================================
[[deprecated("Use new_function() instead")]]
void old_function() {}

void showcase_attributes() {
    banner("Attributes");

    std::cout << "  [[maybe_unused]]: suppresses unused warnings\n";
    std::cout << "  [[nodiscard]]: warns if return value is discarded\n";
    std::cout << "  [[deprecated]]: marks as deprecated\n";
    std::cout << "  [[likely]] / [[unlikely]]: branch prediction hints (C++20)\n";
    std::cout << "  [[no_unique_address]]: empty base optimization (C++20)\n";
    std::cout << "  [[fallthrough]]: explicit switch fallthrough (C++17)\n";

    // [[likely]] / [[unlikely]] example
    int x = 42;
    if (x > 0) [[likely]] {
        std::cout << "  [[likely]] branch taken\n";
    } else [[unlikely]] {
        std::cout << "  [[unlikely]] branch taken\n";
    }

    // [[no_unique_address]] (C++20)
    struct Empty {};
    struct WithEmpty {
        [[no_unique_address]] Empty e;
        int value;
    };
    struct WithoutEmpty {
        Empty e;
        int value;
    };
    std::cout << "  sizeof(WithEmpty): " << sizeof(WithEmpty)
              << " vs sizeof(WithoutEmpty): " << sizeof(WithoutEmpty) << '\n';
}

// ============================================================================
// SECTION 33: Aggregate Initialization & Designated Initializers
// ============================================================================
struct Config {
    int width = 800;
    int height = 600;
    bool fullscreen = false;
    std::string title = "Default";
};

struct Point3D { double x, y, z; };

void showcase_initialization() {
    banner("Aggregate & Designated Initializers");

    sub("Aggregate initialization");
    Point3D p1{1.0, 2.0, 3.0};
    std::cout << "  p1: " << p1.x << "," << p1.y << "," << p1.z << '\n';

    sub("Designated initializers (C++20)");
    Config cfg{.width = 1920, .height = 1080, .fullscreen = true, .title = "Game"};
    std::cout << "  config: " << cfg.width << "x" << cfg.height
              << " fs=" << cfg.fullscreen << " title=" << cfg.title << '\n';

    // Partial designated init (uses defaults for unspecified)
    Config cfg2{.fullscreen = true};
    std::cout << "  config2: " << cfg2.width << "x" << cfg2.height
              << " fs=" << cfg2.fullscreen << " title=" << cfg2.title << '\n';
}

// ============================================================================
// SECTION 34: Concepts in Practice — Constrained Class & CRTP
// ============================================================================
// CRTP (Curiously Recurring Template Pattern)
template<typename Derived>
struct Comparable {
    bool operator>(const Derived& other) const {
        return static_cast<const Derived*>(this)->value() > other.value();
    }
    bool operator<(const Derived& other) const {
        return static_cast<const Derived*>(this)->value() < other.value();
    }
};

struct Temperature : Comparable<Temperature> {
    double temp;
    explicit Temperature(double t) : temp(t) {}
    double value() const { return temp; }
};

// Constrained class with concepts
template<Arithmetic T>
class Matrix {
    std::vector<T> data_;
    size_t rows_, cols_;
public:
    Matrix(size_t r, size_t c, T init = T{})
        : data_(r * c, init), rows_(r), cols_(c) {}

    T& operator()(size_t r, size_t c) { return data_[r * cols_ + c]; }
    const T& operator()(size_t r, size_t c) const { return data_[r * cols_ + c]; }

    size_t rows() const { return rows_; }
    size_t cols() const { return cols_; }

    Matrix operator+(const Matrix& o) const {
        Matrix result(rows_, cols_);
        for (size_t i = 0; i < data_.size(); ++i)
            result.data_[i] = data_[i] + o.data_[i];
        return result;
    }

    void print(std::string_view label) const {
        std::cout << "  " << label << " (" << rows_ << "x" << cols_ << "):\n";
        for (size_t r = 0; r < rows_; ++r) {
            std::cout << "    ";
            for (size_t c = 0; c < cols_; ++c)
                std::cout << std::format("{:>6.1f} ", static_cast<double>((*this)(r, c)));
            std::cout << '\n';
        }
    }
};

void showcase_advanced_patterns() {
    banner("Advanced Patterns: CRTP & Constrained Classes");

    sub("CRTP");
    Temperature t1(100.0), t2(98.6);
    std::cout << "  100 > 98.6: " << (t1 > t2) << '\n';
    std::cout << "  100 < 98.6: " << (t1 < t2) << '\n';

    sub("Constrained Matrix<double>");
    Matrix<double> m1(2, 3, 1.0);
    Matrix<double> m2(2, 3, 2.0);
    m1(0, 0) = 5.0;
    m2(1, 2) = 9.0;
    auto m3 = m1 + m2;
    m3.print("m1 + m2");
}

// ============================================================================
// SECTION 35: Type Erasure Pattern
// ============================================================================
class AnyPrintable {
    struct Concept {
        virtual ~Concept() = default;
        virtual void print(std::ostream&) const = 0;
        virtual std::unique_ptr<Concept> clone() const = 0;
    };

    template<typename T>
    struct Model : Concept {
        T data;
        Model(T d) : data(std::move(d)) {}
        void print(std::ostream& os) const override { os << data; }
        std::unique_ptr<Concept> clone() const override {
            return std::make_unique<Model>(*this);
        }
    };

    std::unique_ptr<Concept> impl_;

public:
    template<typename T>
    AnyPrintable(T val) : impl_(std::make_unique<Model<T>>(std::move(val))) {}

    AnyPrintable(const AnyPrintable& o) : impl_(o.impl_->clone()) {}
    AnyPrintable(AnyPrintable&&) = default;
    AnyPrintable& operator=(AnyPrintable o) { std::swap(impl_, o.impl_); return *this; }

    friend std::ostream& operator<<(std::ostream& os, const AnyPrintable& ap) {
        ap.impl_->print(os);
        return os;
    }
};

void showcase_type_erasure() {
    banner("Type Erasure Pattern");

    std::vector<AnyPrintable> items;
    items.emplace_back(42);
    items.emplace_back(3.14);
    items.emplace_back("hello"s);
    items.emplace_back('X');

    std::cout << "  type-erased collection: ";
    for (auto& item : items) std::cout << item << " ";
    std::cout << '\n';
}

// ============================================================================
// SECTION 36: Compile-time Strings & Reflection-lite
// ============================================================================
template<size_t N>
struct FixedString {
    char data[N]{};
    constexpr FixedString(const char (&str)[N]) {
        for (size_t i = 0; i < N; ++i) data[i] = str[i];
    }
    constexpr operator std::string_view() const { return {data, N - 1}; }
};

template<FixedString Name>
struct NamedValue {
    int value;
    static constexpr auto name() { return std::string_view(Name); }
};

void showcase_compiletime_strings() {
    banner("Compile-time Strings (NTTP)");

    NamedValue<"health"> hp{100};
    NamedValue<"mana"> mp{50};
    std::cout << "  " << decltype(hp)::name() << " = " << hp.value << '\n';
    std::cout << "  " << decltype(mp)::name() << " = " << mp.value << '\n';
}

// ============================================================================
// SECTION 37: Mixins with Variadic Inheritance
// ============================================================================
template<typename... Mixins>
struct Entity : Mixins... {
    using Mixins::describe...;   // C++17 using-pack
};

struct HasName {
    std::string name = "Entity";
    void describe(int) const { std::cout << "    name: " << name << '\n'; }
};

struct HasHealth {
    int health = 100;
    void describe(double) const { std::cout << "    health: " << health << '\n'; }
};

struct HasPosition {
    float x = 0, y = 0;
    void describe(char) const { std::cout << "    pos: (" << x << "," << y << ")\n"; }
};

void showcase_mixins() {
    banner("Mixins with Variadic Inheritance");

    Entity<HasName, HasHealth, HasPosition> e;
    e.name = "Hero";
    e.health = 95;
    e.x = 10.5f;
    e.y = 20.3f;

    std::cout << "  Entity components:\n";
    e.describe(0);      // HasName
    e.describe(0.0);    // HasHealth
    e.describe('c');    // HasPosition
}

// ============================================================================
// SECTION 38: constexpr Containers & Algorithms
// ============================================================================
constexpr int constexpr_sum() {
    std::array<int, 5> arr{1, 2, 3, 4, 5};
    int total = 0;
    for (auto v : arr) total += v;
    return total;
}

constexpr auto constexpr_sort() {
    std::array<int, 5> arr{5, 3, 1, 4, 2};
    for (size_t i = 0; i < arr.size(); ++i)
        for (size_t j = 0; j + 1 < arr.size() - i; ++j)
            if (arr[j] > arr[j + 1])
                std::swap(arr[j], arr[j + 1]);
    return arr;
}

void showcase_constexpr_containers() {
    banner("constexpr Containers & Algorithms");

    constexpr auto s = constexpr_sum();
    static_assert(s == 15);
    std::cout << "  constexpr sum: " << s << '\n';

    constexpr auto sorted = constexpr_sort();
    std::cout << "  constexpr sorted: ";
    for (auto v : sorted) std::cout << v << " ";
    std::cout << '\n';
    static_assert(sorted[0] == 1 && sorted[4] == 5);
}

// ============================================================================
// SECTION 39: Multidimensional subscript operator (C++23)
// ============================================================================
#if __cpp_multidimensional_subscript >= 202211L
class Grid {
    std::vector<int> data_;
    size_t cols_;
public:
    Grid(size_t rows, size_t cols, int init = 0)
        : data_(rows * cols, init), cols_(cols) {}

    int& operator[](size_t r, size_t c) { return data_[r * cols_ + c]; }
    const int& operator[](size_t r, size_t c) const { return data_[r * cols_ + c]; }
};
#endif

void showcase_multidim_subscript() {
    banner("Multidimensional operator[] (C++23)");

#if __cpp_multidimensional_subscript >= 202211L
    Grid g(3, 4, 0);
    g[1, 2] = 42;
    g[0, 0] = 7;
    std::cout << "  g[1,2] = " << g[1, 2] << '\n';
    std::cout << "  g[0,0] = " << g[0, 0] << '\n';
#else
    std::cout << "  (multidimensional subscript not available)\n";
#endif
}

// ============================================================================
// SECTION 40: Misc Modern Features
// ============================================================================
// Inline variables (C++17)
struct Constants2 {
    static inline constexpr double gravity = 9.81;
    static inline constexpr double speed_of_light = 299'792'458.0;
};

// Nested namespaces (C++17)
namespace Game::Physics::Units {
    constexpr double meters_per_foot = 0.3048;
}

// Spaceship comparison for a complex type
struct Version {
    int major, minor, patch;
    auto operator<=>(const Version&) const = default;

    friend std::ostream& operator<<(std::ostream& os, const Version& v) {
        return os << v.major << "." << v.minor << "." << v.patch;
    }
};

void showcase_misc() {
    banner("Miscellaneous Modern Features");

    sub("Inline variables (C++17)");
    std::cout << "  gravity: " << Constants2::gravity << '\n';
    std::cout << "  speed_of_light: " << Constants2::speed_of_light << '\n';

    sub("Nested namespaces (C++17)");
    std::cout << "  meters_per_foot: " << Game::Physics::Units::meters_per_foot << '\n';

    sub("Three-way comparison");
    Version v1{2, 1, 0}, v2{2, 1, 3};
    std::cout << "  " << v1 << " <=> " << v2 << ": ";
    auto cmp = v1 <=> v2;
    if (cmp < 0) std::cout << "less\n";
    else if (cmp > 0) std::cout << "greater\n";
    else std::cout << "equal\n";

    sub("std::exchange (C++14)");
    int old_val = 42;
    int new_val = std::exchange(old_val, 99);
    std::cout << "  exchange: old=" << new_val << " new=" << old_val << '\n';

    sub("std::to_underlying (C++23)");
#if __cpp_lib_to_underlying >= 202102L
    enum class MyEnum : uint16_t { A = 1, B = 2, C = 3 };
    std::cout << "  to_underlying(B): " << std::to_underlying(MyEnum::B) << '\n';
#else
    std::cout << "  (to_underlying requires C++23)\n";
#endif
}

// ============================================================================
// MAIN — Run all showcases
// ============================================================================
extern "C" int cpp_main() {
    std::cout << R"(
    +===================================================================+
    |          C++ COMPREHENSIVE FEATURE SHOWCASE                       |
    |          Features from C++11 through C++23                        |
    +===================================================================+
)" << '\n';

    showcase_fundamentals();            //  1: Types & literals
    showcase_auto_and_bindings();       //  2: auto, decltype, structured bindings
    showcase_enumerations();            //  3: Enums
    showcase_control_flow();            //  4: if-init, constexpr-if, range-for
    showcase_functions();               //  5: Overloading, constexpr, consteval, fold
    showcase_lambdas();                 //  6: Lambdas (C++11-C++23)
    showcase_classes_raii();            //  7: RAII, Rule of Five
    showcase_inheritance();             //  8: Polymorphism, RTTI, multiple inheritance
    showcase_operators();               //  9: Operator overloading, <=>
    showcase_templates();               // 10: Templates -- basic to advanced
    showcase_concepts();                // 11: Concepts & constraints
    showcase_containers();              // 12: STL containers
    showcase_algorithms();              // 13: Algorithms
    showcase_ranges();                  // 14: Ranges & views
    showcase_smart_pointers();          // 15: Smart pointers
    showcase_vocabulary_types();        // 16: optional, variant, any
    showcase_tuples();                  // 17: Tuples & pairs
    showcase_type_traits();             // 18: Type traits
    showcase_constexpr();               // 19: constexpr, consteval, source_location
    showcase_exceptions();              // 20: Exceptions
    showcase_move_semantics();          // 21: Move semantics & forwarding
    showcase_callables();               // 22: std::function, bind, invoke
    showcase_strings();                 // 23: Strings & formatting
    showcase_regex();                   // 24: Regular expressions
    showcase_concurrency();             // 25: Threads, mutex, atomics, latch, barrier
    showcase_coroutines();              // 26: Coroutines
    showcase_chrono();                  // 27: Chrono & time
    showcase_random();                  // 28: Random numbers
    showcase_filesystem();              // 29: Filesystem
    showcase_bits();                    // 30: Bit manipulation
    showcase_numbers();                 // 31: Numeric constants
    showcase_attributes();              // 32: Attributes
    showcase_initialization();          // 33: Aggregate & designated init
    showcase_advanced_patterns();       // 34: CRTP, constrained classes
    showcase_type_erasure();            // 35: Type erasure
    showcase_compiletime_strings();     // 36: Compile-time strings (NTTP)
    showcase_mixins();                  // 37: Mixin pattern
    showcase_constexpr_containers();    // 38: constexpr containers
    showcase_multidim_subscript();      // 39: Multidimensional subscript
    showcase_misc();                    // 40: Misc modern features

    std::cout << "\n+==============================================================+\n";
    std::cout << "|  ALL 40 SECTIONS COMPLETED SUCCESSFULLY!                     |\n";
    std::cout << "+==============================================================+\n";

    return 0;
}
