// =============================================================================
//  rust_showcase.rs — A comprehensive tour of Rust language features
//  Compile:  rustc rust_showcase.rs -o rust_showcase
//  Run:      ./rust_showcase
// =============================================================================

#![allow(dead_code, unused_variables, unused_mut, unused_imports)]
#![warn(clippy::all)]

// ─────────────────────────────────────────────────────────────────────────────
// 1. MODULES & VISIBILITY
// ─────────────────────────────────────────────────────────────────────────────
mod geometry {
    /// A point in 2-D space.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    impl Point {
        pub fn new(x: f64, y: f64) -> Self {
            Self { x, y }
        }

        pub fn distance(&self, other: &Point) -> f64 {
            ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
        }
    }

    pub mod shapes {
        use super::Point;

        pub trait Area {
            fn area(&self) -> f64;
            fn perimeter(&self) -> f64;
            fn describe(&self) -> String {
                format!(
                    "area={:.2}, perimeter={:.2}",
                    self.area(),
                    self.perimeter()
                )
            }
        }

        pub struct Circle {
            pub center: Point,
            pub radius: f64,
        }

        pub struct Rectangle {
            pub top_left: Point,
            pub width: f64,
            pub height: f64,
        }

        impl Area for Circle {
            fn area(&self) -> f64 {
                std::f64::consts::PI * self.radius * self.radius
            }
            fn perimeter(&self) -> f64 {
                2.0 * std::f64::consts::PI * self.radius
            }
        }

        impl Area for Rectangle {
            fn area(&self) -> f64 {
                self.width * self.height
            }
            fn perimeter(&self) -> f64 {
                2.0 * (self.width + self.height)
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. ENUMS, PATTERN MATCHING & OPTION / RESULT
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
    RegularPolygon { sides: u32, side_len: f64 },
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Triangle { base, height } => 0.5 * base * height,
            Shape::RegularPolygon { sides, side_len } => {
                let n = *sides as f64;
                (n * side_len * side_len) / (4.0 * (std::f64::consts::PI / n).tan())
            }
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Shape::Circle { .. } => "Circle",
            Shape::Rectangle { .. } => "Rectangle",
            Shape::Triangle { .. } => "Triangle",
            Shape::RegularPolygon { sides: 3, .. } => "Equilateral Triangle",
            Shape::RegularPolygon { sides: 6, .. } => "Hexagon",
            Shape::RegularPolygon { .. } => "Polygon",
        }
    }
}

fn parse_number(s: &str) -> Result<f64, String> {
    s.trim()
        .parse::<f64>()
        .map_err(|e| format!("parse error: {e}"))
}

fn find_first_positive(nums: &[i32]) -> Option<i32> {
    nums.iter().copied().find(|&n| n > 0)
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. GENERICS, TRAITS & TRAIT BOUNDS
// ─────────────────────────────────────────────────────────────────────────────

use std::fmt::{Debug, Display};
use std::ops::Add;

trait Summary {
    fn summarize_author(&self) -> String;
    fn summarize(&self) -> String {
        format!("(Read more from {}...)", self.summarize_author())
    }
}

#[derive(Debug)]
struct Article {
    title: String,
    author: String,
    content: String,
}

impl Summary for Article {
    fn summarize_author(&self) -> String {
        self.author.clone()
    }
    fn summarize(&self) -> String {
        format!("{}, by {} — {}", self.title, self.author, &self.content[..self.content.len().min(40)])
    }
}

/// Generic function with multiple trait bounds
fn print_summary<T: Summary + Debug>(item: &T) {
    println!("  [debug]   {:?}", item);
    println!("  [summary] {}", item.summarize());
}

/// Generic struct
#[derive(Debug)]
struct Pair<T> {
    first: T,
    second: T,
}

impl<T: Display + PartialOrd> Pair<T> {
    fn new(first: T, second: T) -> Self {
        Self { first, second }
    }

    fn larger(&self) -> &T {
        if self.first >= self.second { &self.first } else { &self.second }
    }
}

/// Generic function returning a value (monomorphisation demo)
fn largest<T: PartialOrd>(list: &[T]) -> &T {
    let mut largest = &list[0];
    for item in list {
        if item > largest { largest = item; }
    }
    largest
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. LIFETIMES
// ─────────────────────────────────────────────────────────────────────────────

fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() >= y.len() { x } else { y }
}

struct Important<'a> {
    part: &'a str,
}

impl<'a> Display for Important<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Important: '{}'", self.part)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. CLOSURES, ITERATORS & FUNCTIONAL PATTERNS
// ─────────────────────────────────────────────────────────────────────────────

fn apply_twice<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(f(x))
}

fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n           // closure capturing by move
}

fn demonstrate_iterators() {
    let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    // chained adapters
    let result: Vec<i32> = numbers
        .iter()
        .filter(|&&x| x % 2 == 0)
        .map(|&x| x * x)
        .collect();
    println!("  even squares: {:?}", result);

    // fold / reduce
    let sum: i32 = numbers.iter().sum();
    let product: i32 = numbers.iter().product();
    println!("  sum={sum}, product={product}");

    // zip & enumerate
    let letters = vec!['a', 'b', 'c'];
    let zipped: Vec<_> = numbers.iter().zip(letters.iter()).collect();
    println!("  zipped (first 3): {:?}", &zipped[..3]);

    // flat_map
    let words = vec!["hello world", "foo bar"];
    let chars: Vec<&str> = words.iter().flat_map(|s| s.split_whitespace()).collect();
    println!("  flat_map words: {:?}", chars);

    // custom iterator via take / skip / chain
    let chain: Vec<i32> = (1..=3).chain(8..=10).collect();
    println!("  chain: {:?}", chain);
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. OWNERSHIP, BORROWING & SLICES
// ─────────────────────────────────────────────────────────────────────────────

fn first_word(s: &str) -> &str {
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b' ' { return &s[..i]; }
    }
    s
}

fn ownership_demo() {
    let s1 = String::from("hello");
    let s2 = s1.clone();          // deep copy — s1 still valid
    let len = calculate_length(&s1); // borrow
    println!("  s1={s1}, s2={s2}, len={len}");

    let mut s = String::from("hello");
    change(&mut s);               // mutable borrow
    println!("  after change: {s}");

    // slice types
    let arr = [1, 2, 3, 4, 5];
    let slice: &[i32] = &arr[1..3];
    println!("  array slice: {:?}", slice);
}

fn calculate_length(s: &String) -> usize { s.len() }
fn change(s: &mut String) { s.push_str(", world"); }

// ─────────────────────────────────────────────────────────────────────────────
// 7. STRUCTS, METHODS & OPERATOR OVERLOADING
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vec2 {
    x: f64,
    y: f64,
}

impl Vec2 {
    const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    fn new(x: f64, y: f64) -> Self { Self { x, y } }
    fn length(&self) -> f64 { (self.x * self.x + self.y * self.y).sqrt() }
    fn dot(&self, other: &Vec2) -> f64 { self.x * other.x + self.y * other.y }
    fn normalize(&self) -> Self {
        let len = self.length();
        Self { x: self.x / len, y: self.y / len }
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2 { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl Display for Vec2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({:.2}, {:.2})", self.x, self.y)
    }
}

impl Default for Vec2 {
    fn default() -> Self { Vec2::ZERO }
}

// ─────────────────────────────────────────────────────────────────────────────
// 8. SMART POINTERS: Box, Rc, RefCell, Cell
// ─────────────────────────────────────────────────────────────────────────────

use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Debug)]
enum List {
    Cons(i32, Box<List>),
    Nil,
}

fn smart_pointers_demo() {
    // Box — heap allocation / recursive types
    let list = List::Cons(1, Box::new(List::Cons(2, Box::new(List::Nil))));
    println!("  box list: {:?}", list);

    // Rc — shared ownership
    let a = Rc::new(vec![1, 2, 3]);
    let b = Rc::clone(&a);
    println!("  rc refs={}, a={:?}, b={:?}", Rc::strong_count(&a), a, b);

    // RefCell — interior mutability
    let data = RefCell::new(vec![1, 2, 3]);
    data.borrow_mut().push(4);
    println!("  refcell: {:?}", data.borrow());

    // Cell — Copy types with interior mutability
    let flag = Cell::new(false);
    flag.set(true);
    println!("  cell flag: {}", flag.get());

    // Rc<RefCell<T>> — shared + mutable
    let shared = Rc::new(RefCell::new(0));
    let clone1 = Rc::clone(&shared);
    *clone1.borrow_mut() += 10;
    println!("  Rc<RefCell>: {}", shared.borrow());
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. TRAIT OBJECTS & DYNAMIC DISPATCH
// ─────────────────────────────────────────────────────────────────────────────

trait Animal: Debug {
    fn name(&self) -> &str;
    fn sound(&self) -> &str;
    fn speak(&self) { println!("  {} says {}", self.name(), self.sound()); }
}

#[derive(Debug)] struct Dog { name: String }
#[derive(Debug)] struct Cat { name: String }
#[derive(Debug)] struct Parrot { name: String, phrase: String }

impl Animal for Dog    { fn name(&self)->&str{&self.name} fn sound(&self)->&str{"Woof"} }
impl Animal for Cat    { fn name(&self)->&str{&self.name} fn sound(&self)->&str{"Meow"} }
impl Animal for Parrot {
    fn name(&self) -> &str { &self.name }
    fn sound(&self) -> &str { &self.phrase }
}

fn make_all_speak(animals: &[Box<dyn Animal>]) {
    for a in animals { a.speak(); }
}

// ─────────────────────────────────────────────────────────────────────────────
// 10. ERROR HANDLING — custom error types, ? operator, From
// ─────────────────────────────────────────────────────────────────────────────

use std::fmt;
use std::num::ParseIntError;

#[derive(Debug)]
enum AppError {
    ParseError(ParseIntError),
    NegativeNumber(i64),
    TooBig(i64),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::ParseError(e)    => write!(f, "parse error: {e}"),
            AppError::NegativeNumber(n) => write!(f, "negative number: {n}"),
            AppError::TooBig(n)         => write!(f, "number too big: {n}"),
        }
    }
}

impl From<ParseIntError> for AppError {
    fn from(e: ParseIntError) -> Self { AppError::ParseError(e) }
}

fn validate(s: &str) -> Result<i64, AppError> {
    let n: i64 = s.trim().parse::<i64>()?; // ? uses From<ParseIntError>
    if n < 0  { return Err(AppError::NegativeNumber(n)); }
    if n > 100 { return Err(AppError::TooBig(n)); }
    Ok(n)
}

// ─────────────────────────────────────────────────────────────────────────────
// 11. ITERATORS — implementing the Iterator trait
// ─────────────────────────────────────────────────────────────────────────────

struct Fibonacci {
    a: u64,
    b: u64,
}

impl Fibonacci {
    fn new() -> Self { Fibonacci { a: 0, b: 1 } }
}

impl Iterator for Fibonacci {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        let next = self.a + self.b;
        self.a = self.b;
        self.b = next;
        Some(self.a)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 12. CONCURRENCY — threads, channels, Arc<Mutex<T>>
// ─────────────────────────────────────────────────────────────────────────────

use std::sync::{Arc, Mutex};
use std::thread;
use std::sync::mpsc;

fn concurrency_demo() {
    // Basic thread spawn + join
    let handle = thread::spawn(|| {
        let sum: u64 = (1..=1_000_000).sum();
        sum
    });
    println!("  thread sum 1..1M = {}", handle.join().unwrap());

    // Arc<Mutex<T>> — shared mutable state
    let counter = Arc::new(Mutex::new(0u32));
    let mut handles = vec![];
    for _ in 0..4 {
        let c = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            let mut lock = c.lock().unwrap();
            *lock += 25;
        }));
    }
    for h in handles { h.join().unwrap(); }
    println!("  arc/mutex counter (4×25) = {}", *counter.lock().unwrap());

    // mpsc channel
    let (tx, rx) = mpsc::channel::<String>();
    let tx2 = tx.clone();
    thread::spawn(move || { tx.send("hello from thread 1".to_string()).unwrap(); });
    thread::spawn(move || { tx2.send("hello from thread 2".to_string()).unwrap(); });
    for _ in 0..2 {
        println!("  channel received: {}", rx.recv().unwrap());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 13. MACROS
// ─────────────────────────────────────────────────────────────────────────────

/// Declarative macro — a mini `vec!` for `HashMap`
macro_rules! map {
    ($($k:expr => $v:expr),* $(,)?) => {{
        let mut m = std::collections::HashMap::new();
        $( m.insert($k, $v); )*
        m
    }};
}

/// Variadic max macro
macro_rules! max {
    ($x:expr) => ($x);
    ($x:expr, $($rest:expr),+) => {
        std::cmp::max($x, max!($($rest),+))
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// 14. STRING HANDLING
// ─────────────────────────────────────────────────────────────────────────────

fn string_demo() {
    // &str vs String
    let s_literal: &str = "hello, world";
    let s_owned: String = s_literal.to_uppercase();
    let s_formatted = format!("{} — len={}", s_owned, s_owned.len());

    // slicing, splitting, collecting
    let csv = "one,two,three,four";
    let parts: Vec<&str> = csv.split(',').collect();
    println!("  csv parts: {:?}", parts);

    // bytes, chars, grapheme clusters (chars here)
    let emoji = "Hello 🌍!";
    println!("  '{}' — {} chars, {} bytes", emoji, emoji.chars().count(), emoji.len());

    // String building
    let mut built = String::with_capacity(64);
    for word in &parts { built.push_str(word); built.push(' '); }
    println!("  built: '{}'", built.trim());

    // Pattern matching on strings
    let greeting = "Hi there";
    let resp = match greeting {
        s if s.starts_with("Hi")    => "Hey!",
        s if s.starts_with("Hello") => "Greetings!",
        _                           => "...",
    };
    println!("  response: {resp}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 15. COLLECTIONS — HashMap, HashSet, BTreeMap, VecDeque
// ─────────────────────────────────────────────────────────────────────────────

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

fn collections_demo() {
    // HashMap with entry API
    let mut scores: HashMap<&str, u32> = HashMap::new();
    for name in ["Alice", "Bob", "Alice", "Carol", "Bob", "Alice"] {
        *scores.entry(name).or_insert(0) += 1;
    }
    println!("  word counts: {:?}", scores);

    // macro-built map
    let capitals = map! {
        "Sweden"  => "Stockholm",
        "Germany" => "Berlin",
        "Japan"   => "Tokyo",
    };
    println!("  capitals: {:?}", capitals);

    // HashSet operations
    let a: HashSet<i32> = [1, 2, 3, 4].iter().cloned().collect();
    let b: HashSet<i32> = [3, 4, 5, 6].iter().cloned().collect();
    let mut inter: Vec<i32> = a.intersection(&b).cloned().collect();
    inter.sort();
    println!("  intersection: {:?}", inter);

    // BTreeMap (sorted)
    let mut btree: BTreeMap<&str, i32> = BTreeMap::new();
    btree.insert("banana", 3); btree.insert("apple", 1); btree.insert("cherry", 2);
    println!("  sorted btree: {:?}", btree);

    // VecDeque — efficient front/back operations
    let mut deque: VecDeque<i32> = (1..=5).collect();
    deque.push_front(0);
    deque.push_back(6);
    println!("  deque: {:?}", deque);
}

// ─────────────────────────────────────────────────────────────────────────────
// 16. TYPE SYSTEM — type aliases, newtype pattern, From/Into, TryFrom
// ─────────────────────────────────────────────────────────────────────────────

type Meters = f64;
type Kilograms = f64;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct Celsius(f64);

#[derive(Debug, Clone, Copy)]
struct Fahrenheit(f64);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self { Fahrenheit(c.0 * 9.0 / 5.0 + 32.0) }
}

impl From<Fahrenheit> for Celsius {
    fn from(f: Fahrenheit) -> Self { Celsius((f.0 - 32.0) * 5.0 / 9.0) }
}

impl Display for Celsius    { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{:.1}°C", self.0) } }
impl Display for Fahrenheit { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{:.1}°F", self.0) } }

use std::convert::TryFrom;

#[derive(Debug)]
struct EvenNumber(i32);

impl TryFrom<i32> for EvenNumber {
    type Error = String;
    fn try_from(n: i32) -> Result<Self, Self::Error> {
        if n % 2 == 0 { Ok(EvenNumber(n)) }
        else           { Err(format!("{n} is not even")) }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 17. UNSAFE RUST
// ─────────────────────────────────────────────────────────────────────────────

fn unsafe_demo() {
    // Raw pointer arithmetic
    let mut value: i32 = 42;
    let r = &value as *const i32;
    let rw = &mut value as *mut i32;

    unsafe {
        println!("  raw ptr *r = {}", *r);
        *rw = 100;
        println!("  after write *r = {}", *r);
    }

    // Calling an unsafe function
    unsafe fn dangerous() -> i32 { 7 }
    let result = unsafe { dangerous() };
    println!("  unsafe fn result: {result}");

    // FFI call — abs from C stdlib
    unsafe extern "C" { fn abs(n: i32) -> i32; }
    let neg = -99i32;
    println!("  ffi abs({neg}) = {}", unsafe { abs(neg) });
}

// ─────────────────────────────────────────────────────────────────────────────
// 18. ASSOCIATED TYPES & WHERE CLAUSES
// ─────────────────────────────────────────────────────────────────────────────

trait Container {
    type Item;
    fn first(&self) -> Option<&Self::Item>;
    fn last(&self)  -> Option<&Self::Item>;
    fn len(&self)   -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }
}

struct Stack<T> { data: Vec<T> }

impl<T> Stack<T> {
    fn new() -> Self { Stack { data: Vec::new() } }
    fn push(&mut self, v: T) { self.data.push(v); }
    fn pop(&mut self) -> Option<T> { self.data.pop() }
}

impl<T> Container for Stack<T> {
    type Item = T;
    fn first(&self) -> Option<&T> { self.data.first() }
    fn last(&self)  -> Option<&T> { self.data.last() }
    fn len(&self)   -> usize      { self.data.len() }
}

fn print_container<C>(c: &C)
where
    C: Container,
    C::Item: Debug + Display,
{
    println!("  container len={}, first={:?}, last={:?}",
        c.len(), c.first(), c.last());
}

// ─────────────────────────────────────────────────────────────────────────────
// 19. BUILDER PATTERN
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Config {
    host:       String,
    port:       u16,
    max_conns:  u32,
    tls:        bool,
}

struct ConfigBuilder {
    host:       String,
    port:       u16,
    max_conns:  u32,
    tls:        bool,
}

impl ConfigBuilder {
    fn new() -> Self {
        ConfigBuilder { host: "localhost".into(), port: 8080, max_conns: 100, tls: false }
    }
    fn host(mut self, h: &str)    -> Self { self.host = h.to_string(); self }
    fn port(mut self, p: u16)     -> Self { self.port = p; self }
    fn max_conns(mut self, n: u32)-> Self { self.max_conns = n; self }
    fn tls(mut self, t: bool)     -> Self { self.tls = t; self }
    fn build(self) -> Config {
        Config { host: self.host, port: self.port, max_conns: self.max_conns, tls: self.tls }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 20. ADVANCED PATTERN MATCHING — guards, binding, destructuring
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
    ChangeColor(u8, u8, u8),
}

fn process_message(msg: &Message) -> String {
    match msg {
        Message::Quit                     => "quit".to_string(),
        Message::Move { x, y } if *x == 0 && *y == 0
                                          => "move to origin".to_string(),
        Message::Move { x, y }            => format!("move to ({x},{y})"),
        Message::Write(text)              => format!("write: {text}"),
        Message::ChangeColor(r, g, b)     => format!("color rgb({r},{g},{b})"),
    }
}

fn advanced_destructuring() {
    // tuple destructuring
    let (a, b, c) = (1, "two", 3.0_f64);
    println!("  tuple: a={a}, b={b}, c={c}");

    // struct destructuring
    let p = geometry::Point::new(3.0, 4.0);
    let geometry::Point { x, y } = p;
    println!("  point: x={x}, y={y}");

    // nested + @ bindings
    let nums = [1, 2, 3, 4, 5];
    let [first, .., last] = nums;
    println!("  slice pattern: first={first}, last={last}");

    let n = 15_u32;
    let desc = match n {
        x @ 1..=9   => format!("single digit {x}"),
        x @ 10..=99 => format!("double digit {x}"),
        x           => format!("large {x}"),
    };
    println!("  n={n}: {desc}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 21. CONST & STATIC, CONST GENERICS
// ─────────────────────────────────────────────────────────────────────────────

const MAX_POINTS: u32 = 100_000;
static HELLO_WORLD: &str = "Hello, Rust!";

/// Const-generic array wrapper
#[derive(Debug)]
struct Grid<const W: usize, const H: usize> {
    cells: [[u8; W]; H],
}

impl<const W: usize, const H: usize> Grid<W, H> {
    fn new() -> Self { Grid { cells: [[0; W]; H] } }
    fn set(&mut self, row: usize, col: usize, val: u8) { self.cells[row][col] = val; }
    fn get(&self, row: usize, col: usize) -> u8 { self.cells[row][col] }
}

// ─────────────────────────────────────────────────────────────────────────────
// 22. TRAIT IMPLEMENTATIONS: Display, From, Default, Clone, PartialEq, Hash
// ─────────────────────────────────────────────────────────────────────────────

use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
struct Color {
    r: u8, g: u8, b: u8,
}

impl Color {
    fn new(r: u8, g: u8, b: u8) -> Self { Color { r, g, b } }
    fn to_hex(&self) -> String { format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b) }
}

impl Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rgb({},{},{})", self.r, self.g, self.b)
    }
}

impl Default for Color {
    fn default() -> Self { Color { r: 0, g: 0, b: 0 } }
}

impl PartialEq for Color {
    fn eq(&self, other: &Self) -> bool {
        self.r == other.r && self.g == other.g && self.b == other.b
    }
}

impl Eq for Color {}

impl Hash for Color {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.r.hash(state); self.g.hash(state); self.b.hash(state);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MAIN — exercise every section
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    section("1. MODULES & TRAITS (geometry)");
    {
        use geometry::Point;
        use geometry::shapes::{Area, Circle, Rectangle};
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(3.0, 4.0);
        println!("  distance p1→p2 = {:.2}", p1.distance(&p2));
        let c = Circle { center: p1, radius: 5.0 };
        let r = Rectangle { top_left: p1, width: 4.0, height: 6.0 };
        println!("  circle:    {}", c.describe());
        println!("  rectangle: {}", r.describe());
    }

    section("2. ENUMS & PATTERN MATCHING");
    {
        let shapes = vec![
            Shape::Circle { radius: 3.0 },
            Shape::Rectangle { width: 4.0, height: 5.0 },
            Shape::Triangle { base: 6.0, height: 8.0 },
            Shape::RegularPolygon { sides: 6, side_len: 2.0 },
        ];
        for s in &shapes {
            println!("  {:15} area = {:.2}", s.name(), s.area());
        }
        println!("  parse '3.14'  → {:?}", parse_number("3.14"));
        println!("  parse 'bad'   → {:?}", parse_number("bad"));
        println!("  first_positive([−3,−1,5,2]) → {:?}", find_first_positive(&[-3,-1,5,2]));
    }

    section("3. GENERICS, TRAITS, BOUNDS");
    {
        let article = Article {
            title:   "Rust 2024 Edition".to_string(),
            author:  "The Team".to_string(),
            content: "Major improvements land in the 2024 edition of Rust.".to_string(),
        };
        print_summary(&article);

        let pair = Pair::new(5, 10);
        println!("  larger of ({},{}) = {}", pair.first, pair.second, pair.larger());

        let numbers = vec![34, 50, 25, 100, 65];
        println!("  largest in {:?} = {}", numbers, largest(&numbers));
    }

    section("4. LIFETIMES");
    {
        let s1 = String::from("long string is long");
        let result;
        {
            let s2 = String::from("xyz");
            result = longest(s1.as_str(), s2.as_str());
            println!("  longest = '{result}'");
        }
        let novel = String::from("Call me Ishmael. Some years ago...");
        let first = novel.split('.').next().expect("no sentence");
        let imp = Important { part: first };
        println!("  {imp}");
    }

    section("5. CLOSURES & ITERATORS");
    {
        let double = |x: i32| x * 2;
        println!("  apply_twice(double, 3) = {}", apply_twice(double, 3));
        let add5 = make_adder(5);
        println!("  add5(10) = {}", add5(10));
        demonstrate_iterators();
        // custom iterator
        let fibs: Vec<u64> = Fibonacci::new().take(10).collect();
        println!("  fibonacci(10): {:?}", fibs);
    }

    section("6. OWNERSHIP & BORROWING");
    ownership_demo();

    section("7. STRUCTS & OPERATOR OVERLOADING");
    {
        let a = Vec2::new(3.0, 4.0);
        let b = Vec2::new(1.0, 2.0);
        println!("  a={a}, b={b}");
        println!("  a+b = {}", a + b);
        println!("  |a| = {:.2}", a.length());
        println!("  a·b = {:.2}", a.dot(&b));
        println!("  â   = {}", a.normalize());
        println!("  default = {}", Vec2::default());
    }

    section("8. SMART POINTERS");
    smart_pointers_demo();

    section("9. TRAIT OBJECTS & DYNAMIC DISPATCH");
    {
        let animals: Vec<Box<dyn Animal>> = vec![
            Box::new(Dog    { name: "Rex".to_string() }),
            Box::new(Cat    { name: "Whiskers".to_string() }),
            Box::new(Parrot { name: "Polly".to_string(), phrase: "Pieces of eight!".to_string() }),
        ];
        make_all_speak(&animals);
    }

    section("10. ERROR HANDLING");
    {
        for input in ["42", "-5", "200", "abc"] {
            match validate(input) {
                Ok(n)  => println!("  '{}' → Ok({})", input, n),
                Err(e) => println!("  '{}' → Err({})", input, e),
            }
        }
    }

    section("11. CUSTOM ITERATOR (Fibonacci)");
    {
        let sum: u64 = Fibonacci::new()
            .take_while(|&n| n < 1000)
            .filter(|n| n % 2 == 0)
            .sum();
        println!("  sum of even fibonacci < 1000 = {sum}");
    }

    section("12. CONCURRENCY");
    concurrency_demo();

    section("13. MACROS");
    {
        println!("  max!(1,9,3,7,2) = {}", max!(1, 9, 3, 7, 2));
        let m = map!["a" => 1, "b" => 2, "c" => 3];
        let mut keys: Vec<&&str> = m.keys().collect();
        keys.sort();
        println!("  map keys: {:?}", keys);
    }

    section("14. STRING HANDLING");
    string_demo();

    section("15. COLLECTIONS");
    collections_demo();

    section("16. TYPE CONVERSIONS (From/Into/TryFrom)");
    {
        let boiling = Celsius(100.0);
        let f: Fahrenheit = boiling.into();
        println!("  {} = {}", boiling, f);
        let body_temp = Fahrenheit(98.6);
        let c: Celsius = body_temp.into();
        println!("  {} = {}", body_temp, c);

        println!("  TryFrom 4  → {:?}", EvenNumber::try_from(4));
        println!("  TryFrom 7  → {:?}", EvenNumber::try_from(7));
    }

    section("17. UNSAFE RUST");
    unsafe_demo();

    section("18. ASSOCIATED TYPES & WHERE CLAUSES");
    {
        let mut stack: Stack<i32> = Stack::new();
        for i in 1..=5 { stack.push(i * 10); }
        print_container(&stack);
        println!("  popped: {:?}", stack.pop());
        print_container(&stack);
    }

    section("19. BUILDER PATTERN");
    {
        let cfg = ConfigBuilder::new()
            .host("example.com")
            .port(443)
            .max_conns(500)
            .tls(true)
            .build();
        println!("  config: {:?}", cfg);
    }

    section("20. ADVANCED PATTERN MATCHING");
    {
        let messages = [
            Message::Quit,
            Message::Move { x: 0, y: 0 },
            Message::Move { x: 3, y: -2 },
            Message::Write("hello".to_string()),
            Message::ChangeColor(255, 128, 0),
        ];
        for m in &messages {
            println!("  {:?} → {}", m, process_message(m));
        }
        advanced_destructuring();
    }

    section("21. CONST GENERICS & STATICS");
    {
        println!("  MAX_POINTS = {MAX_POINTS}");
        println!("  HELLO_WORLD = {HELLO_WORLD}");
        let mut grid: Grid<4, 3> = Grid::new();
        grid.set(1, 2, 9);
        println!("  grid[1][2] = {}", grid.get(1, 2));
        println!("  grid: {:?}", grid);
    }

    section("22. RICH TRAIT IMPLS (Color)");
    {
        let red = Color::new(255, 0, 0);
        let green = Color::new(0, 255, 0);
        let default_color = Color::default();
        println!("  red   = {red}  hex={}", red.to_hex());
        println!("  green = {green}  hex={}", green.to_hex());
        println!("  default = {default_color}");
        println!("  red == red? {}", red == red.clone());
        println!("  red == green? {}", red == green);

        // Use Color as HashMap key (requires Hash + Eq)
        let mut palette: HashMap<Color, &str> = HashMap::new();
        palette.insert(red.clone(), "Red");
        palette.insert(green.clone(), "Green");
        println!("  palette[red] = {:?}", palette.get(&red));
    }

    section("23. ASYNC/AWAIT (tokio runtime)");
    async_demo().await;

    section("60. ANYHOW — ergonomic error handling");
    anyhow_demo();

    section("61. SERDE & SERDE_JSON — serialization");
    serde_demo();

    section("62. ALL CHANNEL TYPES (std + tokio)");
    all_channels_demo().await;

    section("46. STD::MEM — size_of, swap, replace, take");
    mem_demo();

    section("47. BINARYHEAP (priority queue)");
    binary_heap_demo();

    section("48. SATURATING / WRAPPING / CHECKED ARITHMETIC");
    arithmetic_demo();

    section("49. LET-ELSE & IF-LET CHAINS");
    let_else_demo();

    section("50. STD::FMT FORMATTING TRICKS");
    fmt_demo();

    section("51. DEREF, INDEX & CUSTOM SMART POINTER");
    deref_index_demo();

    section("52. STD::ARRAY::FROM_FN & ARRAY METHODS");
    array_demo();

    section("53. STD::IO::CURSOR — in-memory I/O");
    cursor_demo();

    section("54. STD::ENV — environment & args");
    env_demo();

    section("55. PHANTOMDATA & ZERO-SIZED TYPES");
    phantom_demo();

    section("56. CUSTOM ORD & SORTING");
    ordering_demo();

    section("57. ITERATOR CONSTRUCTORS — from_fn, successors, repeat_with");
    iter_constructors_demo();

    section("58. STD::PROCESS::COMMAND");
    process_demo();

    section("59. ALL STANDARD MACROS");
    all_macros_demo();

    section("24. DROP TRAIT");
    {
        {
            let _drop1 = LoggedDrop::new("item1");
            let _drop2 = LoggedDrop::new("item2");
            let _pool = ResourcePool::new("connection-pool");
            _pool.use_resource();
            println!("  entering block...");
        }
        println!("  left block - destructors ran");
    }

    section("25. COW (Clone on Write)");
    {
        let already_upper = "HELLO";
        let mixed = "Hello World";
        println!("  to_uppercase_cow('{}') = {}", already_upper, to_uppercase_cow(already_upper));
        println!("  to_uppercase_cow('{}') = {}", mixed, to_uppercase_cow(mixed));

        let borrowed = Cow::Borrowed("test");
        let owned = Cow::Owned(String::from("test"));
        process_string(borrowed);
        process_string(owned);
    }

    section("26. ADVANCED OPTION/RESULT COMBINATORS");
    combinators_demo();

    section("27. BOX<dyn Any> & DOWNCASTING");
    {
        let circle = CircleAny { radius: 5.0 };
        let rectangle = RectangleAny { width: 10.0, height: 20.0 };
        println!("  downcast(circle) = {}", downcast_demo(&circle));
        println!("  downcast(rectangle) = {}", downcast_demo(&rectangle));
    }

    section("28. ATOMIC TYPES & LOCK-FREE");
    atomic_demo();

    section("29. BINARY DATA");
    binary_demo();

    section("30. TIME & DURATION");
    time_demo();

    section("31. FILE I/O & PATHS");
    file_io_demo();

    section("32. CUSTOM SERIALIZATION");
    {
        let person = Person {
            name: "Alice".to_string(),
            age: 30,
            email: "alice@example.com".to_string(),
        };
        println!("  to_json: {}", person.to_json());

        let json = r#"{"name":"Bob","age":25,"email":"bob@example.com"}"#;
        match Person::from_json(json) {
            Ok(p) => println!("  from_json: {:?} (parsed)", p),
            Err(e) => println!("  from_json error: {}", e),
        }
    }

    section("33. LAZY INITIALIZATION");
    lazy_demo();

    section("34. FUNCTION TRAITS (Fn, FnMut, FnOnce)");
    function_traits_demo();

    section("35. SERDE-LIKE DERIVE");
    {
        let mut user = User::new(1, "alice");
        user.promote_to_admin();
        println!("  {}", user);

        let mut user2 = User::new(2, "bob");
        user2.deactivate();
        println!("  {}", user2);

        println!("  (Full serde would require serde crate)");
    }

    section("36. ADVANCED PATTERN MATCHING");
    advanced_patterns_demo();

    section("37. TRAIT DEFAULT IMPLS");
    {
        let data = Data { value: 21 };
        println!("  {}", data.process());
        println!("  {}", data.process_with_options(false));
        println!("  {}", data.process_with_options(true));
    }

    section("38. TRAIT OBJECTS WITH SUPERTRAITS");
    {
        let report = Report {
            title: "Q4 Report".to_string(),
            content: "Revenue grew 25% in Q4.".to_string(),
        };
        print_item(&report);
    }

    section("39. RECURSIVE DATA STRUCTURES");
    {
        let tree = Tree::new_node(
            Tree::new_node(Tree::new_leaf(), 5, Tree::new_leaf()),
            10,
            Tree::new_node(Tree::new_leaf(), 15, Tree::new_leaf())
        );
        println!("  in-order traversal: {:?}", tree.in_order());
    }

    section("40. ZERO-COST ABSTRACTIONS");
    {
        let adder = Adder;
        let multiplier = Multiplier;

        println!("  generic_calculate(adder, 3, 5) = {}", generic_calculate(&adder, 3, 5));
        println!("  generic_calculate(multiplier, 3, 5) = {}", generic_calculate(&multiplier, 3, 5));
        println!("  dynamic_calculate(adder, 3, 5) = {}", dynamic_calculate(&adder, 3, 5));
        println!("  dynamic_calculate(multiplier, 3, 5) = {}", dynamic_calculate(&multiplier, 3, 5));
    }

    section("41. BENCHMARKING");
    {
        let _ = measure_performance("fibonacci", || {
            let mut fib: Vec<u64> = vec![0, 1];
            for i in 2..80 {
                fib.push(fib[i-1].wrapping_add(fib[i-2]));
            }
            fib[79]
        });
    }

    section("42. RAW IDENTIFIERS");
    raw_identifiers_demo();

    section("43. NEVER TYPE (!)");
    {
        println!("  maybe_never(false) = {}", maybe_never(false));
        println!("  (maybe_never(true) would panic - not called)");
    }

    section("44. SIMD");
    simd_demo();

    section("45. PANIC HOOKS & ERROR RECOVERY");
    panic_hook_demo();

    println!("\n{}", "═".repeat(60));
    println!("  All Rust features demonstrated successfully! 🦀");
    println!("  Total: 62 feature sections");
    println!("{}", "═".repeat(60));
}

// ─────────────────────────────────────────────────────────────────────────────
// 23. ASYNC/AWAIT — real tokio runtime demos
// ─────────────────────────────────────────────────────────────────────────────

async fn fetch_simulated_data(id: u32) -> String {
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    format!("Data from endpoint {id}")
}

async fn async_demo() {
    // Basic async/await
    let result = fetch_simulated_data(1).await;
    println!("  single fetch: {result}");

    // tokio::spawn — concurrent tasks
    let mut handles = Vec::new();
    for id in 1..=4 {
        handles.push(tokio::spawn(async move {
            fetch_simulated_data(id).await
        }));
    }
    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    println!("  spawned 4 tasks: {results:?}");

    // tokio::join! — await multiple futures concurrently
    let (a, b, c) = tokio::join!(
        fetch_simulated_data(10),
        fetch_simulated_data(20),
        fetch_simulated_data(30),
    );
    println!("  tokio::join!: [{a}, {b}, {c}]");

    // tokio::select! — race multiple futures
    tokio::select! {
        val = fetch_simulated_data(100) => println!("  tokio::select! winner: {val}"),
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => println!("  timeout"),
    }

    // tokio::time::timeout
    let fast = tokio::time::timeout(
        tokio::time::Duration::from_secs(1),
        fetch_simulated_data(42),
    ).await;
    println!("  timeout(1s, fast): {:?}", fast.map(|s| s));

    let slow = tokio::time::timeout(
        tokio::time::Duration::from_millis(1),
        async {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            "done"
        },
    ).await;
    println!("  timeout(1ms, slow): {:?}", slow);

    // tokio::sync::mpsc — async channel
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10);
    let tx2 = tx.clone();
    tokio::spawn(async move { tx.send("async msg 1".into()).await.unwrap(); });
    tokio::spawn(async move { tx2.send("async msg 2".into()).await.unwrap(); });
    let mut msgs = Vec::new();
    for _ in 0..2 {
        msgs.push(rx.recv().await.unwrap());
    }
    msgs.sort();
    println!("  tokio mpsc: {msgs:?}");

    // tokio::sync::oneshot — single-value channel
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move { tx.send(42).unwrap(); });
    println!("  tokio oneshot: {}", rx.await.unwrap());

    // tokio::sync::Mutex — async-aware mutex
    let data = std::sync::Arc::new(tokio::sync::Mutex::new(0));
    let mut handles = Vec::new();
    for _ in 0..5 {
        let data = data.clone();
        handles.push(tokio::spawn(async move {
            let mut lock = data.lock().await;
            *lock += 10;
        }));
    }
    for h in handles { h.await.unwrap(); }
    println!("  tokio Mutex (5x10): {}", *data.lock().await);
}

// ─────────────────────────────────────────────────────────────────────────────
// 60. ANYHOW — ergonomic error handling
// ─────────────────────────────────────────────────────────────────────────────

fn anyhow_demo() {
    use anyhow::{anyhow, bail, ensure, Context, Result};

    // anyhow! — create an ad-hoc error
    fn might_fail(ok: bool) -> Result<i32> {
        if ok { Ok(42) } else { Err(anyhow!("something went wrong")) }
    }
    println!("  anyhow! Ok:  {:?}", might_fail(true));
    println!("  anyhow! Err: {:?}", might_fail(false));

    // bail! — early return with error
    fn check_positive(n: i32) -> Result<i32> {
        if n < 0 { bail!("expected positive, got {n}"); }
        Ok(n * 2)
    }
    println!("  bail! 5:  {:?}", check_positive(5));
    println!("  bail! -3: {:?}", check_positive(-3));

    // ensure! — assert-like macro that returns Err instead of panicking
    fn validate_range(n: i32) -> Result<i32> {
        ensure!(n >= 0 && n <= 100, "out of range: {n}");
        Ok(n)
    }
    println!("  ensure! 50:  {:?}", validate_range(50));
    println!("  ensure! 200: {:?}", validate_range(200));

    // .context() — add context to errors from other libraries
    fn parse_port(s: &str) -> Result<u16> {
        let port: u16 = s.parse()
            .context(format!("failed to parse '{s}' as port number"))?;
        ensure!(port > 0, "port must be positive");
        Ok(port)
    }
    println!("  context '443':  {:?}", parse_port("443"));
    println!("  context 'abc':  {:?}", parse_port("abc"));

    // Chaining errors — anyhow preserves the full error chain
    fn outer() -> Result<()> {
        inner().context("outer operation failed")?;
        Ok(())
    }
    fn inner() -> Result<()> {
        Err(anyhow!("inner root cause"))
    }
    if let Err(e) = outer() {
        println!("  error chain:");
        for (i, cause) in e.chain().enumerate() {
            println!("    [{i}] {cause}");
        }
    }

    // downcast — recover the original error type
    fn typed_error() -> Result<()> {
        let err: std::io::Error = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        Err(err).context("loading config")?;
        Ok(())
    }
    if let Err(e) = typed_error() {
        let is_io = e.downcast_ref::<std::io::Error>().is_some();
        println!("  downcast to io::Error: {is_io}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 24. DROP TRAIT (Custom Destructors)
// ─────────────────────────────────────────────────────────────────────────────

struct LoggedDrop {
    name: String,
}

impl LoggedDrop {
    fn new(name: &str) -> Self {
        println!("  [created] {}", name);
        Self { name: name.to_string() }
    }
}

impl Drop for LoggedDrop {
    fn drop(&mut self) {
        println!("  [dropped]  {}", self.name);
    }
}

struct ResourcePool {
    name: String,
    active: std::cell::Cell<bool>,
}

impl ResourcePool {
    fn new(name: &str) -> Self {
        println!("  [pool created] {}", name);
        Self { name: name.to_string(), active: std::cell::Cell::new(true) }
    }

    fn use_resource(&self) {
        if self.active.get() {
            println!("  [pool using]   {}", self.name);
        } else {
            println!("  [pool dead]    {}", self.name);
        }
    }
}

impl Drop for ResourcePool {
    fn drop(&mut self) {
        self.active.set(false);
        println!("  [pool cleanup] {}", self.name);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 25. COW (Clone on Write)
// ─────────────────────────────────────────────────────────────────────────────

use std::borrow::Cow;

fn to_uppercase_cow(s: &str) -> Cow<'_, str> {
    if s.chars().all(|c| c.is_uppercase()) {
        Cow::Borrowed(s)  // No allocation needed
    } else {
        Cow::Owned(s.to_uppercase())  // Allocate only when needed
    }
}

fn process_string<'a>(input: Cow<'a, str>) {
    println!("  processed: '{}' (is_borrowed={})", input, matches!(input, Cow::Borrowed(_)));
}

// ─────────────────────────────────────────────────────────────────────────────
// 26. ADVANCED OPTION & RESULT COMBINATORS
// ─────────────────────────────────────────────────────────────────────────────

fn combinators_demo() {
    // Option combinators
    let opt_some: Option<i32> = Some(42);
    let opt_none: Option<i32> = None;

    println!("  map:");
    println!("    Some(42).map(|x| x * 2) = {:?}", opt_some.map(|x| x * 2));
    println!("    None.map(|x| x * 2)     = {:?}", opt_none.map(|x| x * 2));

    println!("  and_then:");
    let result = opt_some.and_then(|x| if x > 40 { Some(x+1) } else { None });
    println!("    Some(42).and_then(|x| if x > 40 {{ Some(x+1) }} else {{ None }}) = {:?}", result);

    println!("  or_else:");
    println!("    None.or_else(|| Some(99)) = {:?}", opt_none.or_else(|| Some(99)));

    println!("  unwrap_or_else:");
    println!("    None.unwrap_or_else(|| 0) = {}", opt_none.unwrap_or_else(|| 0));

    // Result combinators
    let ok_result: Result<i32, &str> = Ok(10);
    let err_result: Result<i32, &str> = Err("failed");

    println!("  map_err:");
    let map_err_result = err_result.map_err(|e| format!("Error: {}", e));
    println!("    Err('failed').map_err(\"Error\") = {:?}", map_err_result);

    println!("  or:");
    let or_result: Result<i32, &str> = err_result.or(Ok(99));
    println!("    Err('failed').or(Ok(99)) = {:?}", or_result);

    // Transpose
    let opt_res: Option<Result<i32, &str>> = Some(Ok(10));
    let res_opt: Result<Option<i32>, &str> = Ok(Some(10));
    println!("  transpose:");
    println!("    Some(Ok(10)).transpose() = {:?}", opt_res.transpose());
    println!("    Ok(Some(10)).transpose() = {:?}", res_opt.transpose());
}

// ─────────────────────────────────────────────────────────────────────────────
// 27. BOX<dyn Any> & DOWNCASTING
// ─────────────────────────────────────────────────────────────────────────────

use std::any::Any;

trait ShapeAny: Any {
    fn area(&self) -> f64;
    fn as_any(&self) -> &dyn Any;
}

#[derive(Debug)]
struct CircleAny { radius: f64 }
#[derive(Debug)] struct RectangleAny { width: f64, height: f64 }

impl ShapeAny for CircleAny {
    fn area(&self) -> f64 { std::f64::consts::PI * self.radius * self.radius }
    fn as_any(&self) -> &dyn Any { self }
}

impl ShapeAny for RectangleAny {
    fn area(&self) -> f64 { self.width * self.height }
    fn as_any(&self) -> &dyn Any { self }
}

fn downcast_demo(shape: & dyn ShapeAny) -> String {
    if let Some(circle) = shape.as_any().downcast_ref::<CircleAny>() {
        format!("Circle with radius {}", circle.radius)
    } else if let Some(rect) = shape.as_any().downcast_ref::<RectangleAny>() {
        format!("Rectangle {}x{}", rect.width, rect.height)
    } else {
        "Unknown shape".to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 28. ATOMIC TYPES & LOCK-FREE PROGRAMMING
// ─────────────────────────────────────────────────────────────────────────────

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

fn atomic_demo() {
    let atomic_bool = AtomicBool::new(false);
    let atomic_int = AtomicI32::new(0);

    // Compare and swap (CAS)
    let old_value = atomic_bool.compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed).unwrap();
    println!("  CAS: compare_exchange(false, true) = {:?}", old_value);

    // Fetch operations
    let prev = atomic_int.fetch_add(5, Ordering::SeqCst);
    println!("  fetch_add(5): prev={}, now={}", prev, atomic_int.load(Ordering::SeqCst));

    let prev = atomic_int.fetch_sub(2, Ordering::Relaxed);
    println!("  fetch_sub(2): prev={}, now={}", prev, atomic_int.load(Ordering::SeqCst));

    let prev = atomic_int.fetch_max(100, Ordering::Relaxed);
    println!("  fetch_max(100): prev={}, now={}", prev, atomic_int.load(Ordering::SeqCst));
}

// ─────────────────────────────────────────────────────────────────────────────
// 29. BINARY DATA: BYTE ARRAYS, ENDIANESS, BIT MANIPULATION
// ─────────────────────────────────────────────────────────────────────────────

fn binary_demo() {
    // Byte array with explicit types
    let bytes: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];
    println!("  hex bytes: {:02X} {:02X} {:02X} {:02X}", bytes[0], bytes[1], bytes[2], bytes[3]);

    // Endianness conversion
    let num: u32 = 0xDEADBEEF;
    let be_bytes = num.to_be_bytes();
    let le_bytes = num.to_le_bytes();
    println!("  u32=0x{:08X}", num);
    println!("    BE bytes: {:02X} {:02X} {:02X} {:02X}", be_bytes[0], be_bytes[1], be_bytes[2], be_bytes[3]);
    println!("    LE bytes: {:02X} {:02X} {:02X} {:02X}", le_bytes[0], le_bytes[1], le_bytes[2], le_bytes[3]);

    // Bit manipulation
    let mut flags: u8 = 0b0000_0000;
    flags |= 0b0001_0000;  // Set bit 4
    flags &= 0b1110_1111;  // Clear bit 4
    flags ^= 0b0010_0000;  // Toggle bit 5
    println!("  bit ops: flags = 0b{:08b}", flags);
    println!("    bit 4 set? {}", (flags & 0b0001_0000) != 0);
    println!("    bit 5 set? {}", (flags & 0b0010_0000) != 0);

    // Bit shifts
    let x: i32 = 16;
    println!("  bit shifts: {} << 2 = {}; {} >> 1 = {}", x, x << 2, x, x >> 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// 30. TIME & DURATION
// ─────────────────────────────────────────────────────────────────────────────

use std::time::{Duration, Instant};

fn time_demo() {
    let now = Instant::now();
    std::thread::sleep(Duration::from_millis(50));
    let elapsed = now.elapsed();
    println!("  elapsed time: {:?}", elapsed);
    println!("    as millis:  {}ms", elapsed.as_millis());
    println!("    as secs:   {}s", elapsed.as_secs());

    let duration = Duration::from_secs(2) + Duration::from_millis(500);
    println!("  duration: {:?}", duration);
    println!("    human readable: {}s {}ms", duration.as_secs(), duration.subsec_millis());

    // Timestamp (system time)
    let system_time = std::time::SystemTime::now();
    let unix_timestamp = system_time.duration_since(std::time::UNIX_EPOCH).unwrap();
    println!("  unix timestamp: {}s", unix_timestamp.as_secs());
}

// ─────────────────────────────────────────────────────────────────────────────
// 31. FILE I/O & PATHS
// ─────────────────────────────────────────────────────────────────────────────

use std::fs;
use std::path::{Path, PathBuf};

fn file_io_demo() {
    // Path manipulation
    let path = Path::new("/tmp/test/rust_demo.txt");
    println!("  path: {}", path.display());
    println!("    exists?  {}", path.exists());
    println!("    parent?  {:?}", path.parent());
    println!("    file_stem: {:?}", path.file_stem());
    println!("    extension: {:?}", path.extension());

    // PathBuf (owned path)
    let mut path_buf = PathBuf::new();
    path_buf.push("/tmp");
    path_buf.push("test");
    path_buf.set_extension("dat");
    println!("  path_buf: {}", path_buf.display());

    // Writing (demonstration - commented to avoid actual file creation)
    /*
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }

    let content = "Hello from Rust File I/O demo!";
    fs::write(path, content).expect("Failed to write file");

    // Reading
    let read_content = fs::read_to_string(path).expect("Failed to read file");
    println!("  file content: '{}'", read_content);

    // Metadata
    let metadata = fs::metadata(path).expect("Failed to get metadata");
    println!("  file size: {} bytes", metadata.len());

    // Cleanup
    fs::remove_file(path).ok();
    */

    println!("  (file operations commented to avoid actual I/O)");
}

// ─────────────────────────────────────────────────────────────────────────────
// 32. CUSTOM SERIALIZATION (Manual)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Person {
    name: String,
    age: u32,
    email: String,
}

impl Person {
    fn to_json(&self) -> String {
        format!(
            r#"{{"name":"{}","age":{},"email":"{}"}}"#,
            self.name, self.age, self.email
        )
    }

    fn from_json(json: &str) -> Result<Self, String> {
        // Simple JSON parser for demo purposes
        let json = json.trim().trim_start_matches('{').trim_end_matches('}');

        fn extract_field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
            let pattern = format!("\"{}\":\"", key);
            let start = json.find(&pattern)? + pattern.len();
            let end = json[start..].find('"')?;
            Some(&json[start..start + end])
        }

        let name = extract_field(json, "name")
            .ok_or_else(|| "Missing 'name' field".to_string())?
            .to_string();

        let age_str = extract_field(json, "age")
            .ok_or_else(|| "Missing 'age' field".to_string())?;
        let age: u32 = age_str.parse().map_err(|e| format!("Invalid age: {}", e))?;

        let email = extract_field(json, "email")
            .ok_or_else(|| "Missing 'email' field".to_string())?
            .to_string();

        Ok(Person { name, age, email })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 33. LAZY INITIALIZATION & STATIC MUTABLE DATA
// ─────────────────────────────────────────────────────────────────────────────

use std::sync::OnceLock;

static CONFIG: OnceLock<String> = OnceLock::new();

fn get_config() -> &'static str {
    CONFIG.get_or_init(|| {
        "default-config-value".to_string()
    })
}

fn lazy_demo() {
    println!("  config: {}", get_config());
    println!("  config (cached): {}", get_config());

    // Thread-local storage
    thread_local! {
        static COUNTER: std::cell::RefCell<u32> = std::cell::RefCell::new(0);
    }

    COUNTER.with(|counter| {
        *counter.borrow_mut() += 1;
        println!("  thread_local counter: {}", *counter.borrow());
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// 34. FUNCTION TRAITS (Fn, FnMut, FnOnce)
// ─────────────────────────────────────────────────────────────────────────────

fn function_traits_demo() {
    // Fn - can be called multiple times, borrows data
    let numbers = vec![1, 2, 3, 4, 5];
    let first = &numbers[..3];

    let sum: i32 = first.iter().sum();
    println!("  Fn example: sum of {:?} = {}", first, sum);

    // FnMut - can be called multiple times, mutably borrows data
    let mut state = 0;
    let mut mut_closure = || {
        state += 1;
        state
    };
    println!("  FnMut example: {}", mut_closure());
    println!("  FnMut example: {}", mut_closure());

    // FnOnce - can only be called once, consumes data
    let data = vec![1, 2, 3];
    let once_closure = || {
        data.into_iter().sum::<i32>()
    };
    println!("  FnOnce example: sum consumed = {}", once_closure());
    // once_closure(); // This would be a compile error!

    // Higher-order functions
    fn apply<F>(func: F, value: i32) -> i32
    where
        F: Fn(i32) -> i32
    {
        func(value)
    }

    let double = |x| x * 2;
    println!("  higher-order: apply(double, 5) = {}", apply(double, 5));
}

// ─────────────────────────────────────────────────────────────────────────────
// 35. SERDE-LIKE DERIVE MACRO DEMO (using derive macros)
// ─────────────────────────────────────────────────────────────────────────────

// This would normally require serde crate imported
// #[derive(Serialize, Deserialize)]
// #[derive(PartialEq, Eq, Hash, Clone, Debug)]
struct User {
    id: u64,
    username: String,
    is_active: bool,
    roles: Vec<String>,
}

impl User {
    fn new(id: u64, username: &str) -> Self {
        Self {
            id,
            username: username.to_string(),
            is_active: true,
            roles: vec!["user".to_string()],
        }
    }

    fn promote_to_admin(&mut self) {
        if !self.roles.contains(&"admin".to_string()) {
            self.roles.push("admin".to_string());
        }
    }

    fn deactivate(&mut self) {
        self.is_active = false;
    }
}

impl Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "User(id={}, username={}, active={}, roles={:?})",
               self.id, self.username, self.is_active, self.roles)
    }
}

// Note: Real serde usage would require adding serde to Cargo.toml
// #[derive(Debug, Serialize, Deserialize)]
// struct Config {
//     database_url: String,
//     max_connections: u32,
// }

// ─────────────────────────────────────────────────────────────────────────────
// 36. ADVANCED PATTERN MATCHING: OR PATTERNS, RANGES, LITERALS
// ─────────────────────────────────────────────────────────────────────────────

fn advanced_patterns_demo() {
    // OR patterns
    let x = 3;
    match x {
        1 | 2 | 3 => println!("  OR pattern: small (1-3)"),
        _ => println!("  OR pattern: not small"),
    }

    // Range patterns
    let y = 42;
    match y {
        0..=10 => println!("  Range pattern: single digit"),
        11..=99 => println!("  Range pattern: double digit"),
        _ => println!("  Range pattern: three+ digits"),
    }

    // Literal patterns in match arms
    let message = "hello";
    match message {
        "hello" | "hi" | "hey" => println!("  Literal pattern: greeting"),
        "goodbye" | "bye" => println!("  Literal pattern: farewell"),
        _ => println!("  Literal pattern: unknown"),
    }

    // Nested patterns
    let point = (3, 4);
    match point {
        (0, 0) => println!("  Nested: origin"),
        (0, y) => println!("  Nested: on y-axis at {y}"),
        (x, 0) => println!("  Nested: on x-axis at {x}"),
        (x, y) => println!("  Nested: at ({x}, {y})"),
    }

    // @ binding with or patterns
    let num = 7;
    let description = match num {
        n @ 1..=10 => format!("small number {n}"),
        n @ 11..=100 => format!("medium number {n}"),
        n => format!("large number {n}"),
    };
    println!("  @ binding: {}", description);
}

// ─────────────────────────────────────────────────────────────────────────────
// 37. TRAIT DEFAULT implementations with specialized logic
// ─────────────────────────────────────────────────────────────────────────────

trait Processable {
    fn process(&self) -> String;
    fn process_with_options(&self, verbose: bool) -> String {
        if verbose {
            format!("[VERBOSE] {}", self.process())
        } else {
            self.process()
        }
    }
}

struct Data {
    value: i32,
}

impl Processable for Data {
    fn process(&self) -> String {
        format!("processed value: {}", self.value * 2)
    }

    // Specialized implementation
    fn process_with_options(&self, verbose: bool) -> String {
        let val = self.value;
        match verbose {
            true => format!("[VER] value={}, result={}", val, val * 2),
            false => format!("res={}", val * 2),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 38. TRAIT OBJECTS WITH SUPERTRAITS
// ─────────────────────────────────────────────────────────────────────────────

trait Printable: Display {
    fn print_pretty(&self) -> String {
        format!("── {} ──", self)
    }
}

#[derive(Debug)]
struct Report {
    title: String,
    content: String,
}

impl Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Report: {}", self.title)
    }
}

impl Printable for Report {
    fn print_pretty(&self) -> String {
        format!(
            "══════════════════════\n{}\n{}\n══════════════════════",
            self.title, self.content
        )
    }
}

fn print_item(item: &dyn Printable) {
    println!("  {}", item.print_pretty());
}

// ─────────────────────────────────────────────────────────────────────────────
// 39. RECURSIVE DATA STRUCTURES
// ─────────────────────────────────────────────────────────────────────────────

enum Tree<T> {
    Leaf,
    Node(Box<Tree<T>>, T, Box<Tree<T>>),
}

impl<T: Display> Tree<T> {
    fn new_leaf() -> Self {
        Tree::Leaf
    }

    fn new_node(left: Tree<T>, value: T, right: Tree<T>) -> Self {
        Tree::Node(Box::new(left), value, Box::new(right))
    }

    fn in_order(&self) -> Vec<String> {
        match self {
            Tree::Leaf => vec![],
            Tree::Node(left, value, right) => {
                let mut result = left.in_order();
                result.push(value.to_string());
                result.extend(right.in_order());
                result
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 40. ZERO-COST ABSTRACTIONS: Generic vs Trait Object
// ─────────────────────────────────────────────────────────────────────────────

trait Calculate {
    fn compute(&self, x: i32, y: i32) -> i32;
}

struct Adder;
struct Multiplier;

impl Calculate for Adder {
    fn compute(&self, x: i32, y: i32) -> i32 { x + y }
}

impl Calculate for Multiplier {
    fn compute(&self, x: i32, y: i32) -> i32 { x * y }
}

// Generic version - static dispatch, monomorphization
fn generic_calculate<T: Calculate>(calc: &T, x: i32, y: i32) -> i32 {
    calc.compute(x, y)
}

// Trait object version - dynamic dispatch
fn dynamic_calculate(calc: &dyn Calculate, x: i32, y: i32) -> i32 {
    calc.compute(x, y)
}

// ─────────────────────────────────────────────────────────────────────────────
// 41. BENCHMARKING (std::time::Instant)
// ─────────────────────────────────────────────────────────────────────────────

fn measure_performance<F, R>(name: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let duration = start.elapsed();
    println!("  {} took {:?}", name, duration);
    result
}

// ─────────────────────────────────────────────────────────────────────────────
// 42. RAW IDENTIFIERS (for using keywords as names)
// ─────────────────────────────────────────────────────────────────────────────

fn raw_identifiers_demo() {
    let r#fn = 42;  // 'fn' is a keyword, but we can use it as a name with r#
    let r#struct = "hello";
    let r#match = true;

    println!("  raw fn: {}", r#fn);
    println!("  raw struct: {}", r#struct);
    println!("  raw match: {}", r#match);
}

// ─────────────────────────────────────────────────────────────────────────────
// 43. NEVER TYPE (!) AND DIVERGING FUNCTIONS
// ─────────────────────────────────────────────────────────────────────────────

fn never_returns() -> ! {
    panic!("This function never returns!");
}

fn maybe_never(should_panic: bool) -> i32 {
    if should_panic {
        never_returns()  // Known to never return, so compiler knows this path is unreachable
    }
    42
}

// ─────────────────────────────────────────────────────────────────────────────
// 44. SIMD (Single Instruction, Multiple Data) - using portable_simd
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

fn simd_sum_scalar(values: &[i32]) -> i32 {
    values.iter().sum()
}

// Note: This requires nightly Rust and proper feature flags
// #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
// unsafe fn simd_sum_avx2(values: &[i32]) -> i32 {
//     let chunks = values.chunks_exact(8);
//     let remainder = chunks.remainder();
//
//     let mut sum = _mm256_setzero_si256();
//
//     for chunk in chunks {
//         let v = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);
//         sum = _mm256_add_epi32(sum, v);
//     }
//
//     let mut result = [0i32; 8];
//     _mm256_storeu_si256(result.as_mut_ptr() as *mut __m256i, sum);
//
//     let total: i32 = result.iter().chain(remainder).sum();
//     total
// }

fn simd_demo() {
    let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let scalar_result = measure_performance("scalar sum", || simd_sum_scalar(&data));
    println!("    result: {}", scalar_result);

    // SIMD result would be here if enabled
    println!("    (SIMD requires nightly Rust and proper target features)");
}

// ─────────────────────────────────────────────────────────────────────────────
// 45. PANIC HOOKS AND ERROR RECOVERY
// ─────────────────────────────────────────────────────────────────────────────

use std::panic;

fn panic_hook_demo() {
    // Set up custom panic hook (demonstration)
    let original_hook = panic::take_hook();

    panic::set_hook(Box::new(|panic_info| {
        if let Some(location) = panic_info.location() {
            println!("panic occurred at {} {}:{}",
                location.file(),
                location.line(),
                location.column()
            );
        } else {
            println!("panic occurred but can't get location information");
        }
        if let Some(msg) = panic_info.payload().downcast_ref::<&str>() {
            println!("panic message: {}", msg);
        }
    }));

    // Restore original hook
    let _ = panic::take_hook();
    panic::set_hook(original_hook);

    // catch_unwind example
    let result = panic::catch_unwind(|| {
        // This would normally panic
        // panic!("test panic");
        42
    });

    match result {
        Ok(value) => println!("  catch_unwind succeeded: {}", value),
        Err(_) => println!("  catch_unwind caught panic"),
    }
}
// ─────────────────────────────────────────────────────────────────────────────
// 46. STD::MEM — size_of, swap, replace, take
// ─────────────────────────────────────────────────────────────────────────────

fn mem_demo() {
    use std::mem;

    println!("  size_of::<u8>()    = {}", mem::size_of::<u8>());
    println!("  size_of::<u64>()   = {}", mem::size_of::<u64>());
    println!("  size_of::<Vec2>()  = {}", mem::size_of::<Vec2>());
    println!("  size_of::<Option<u8>>()  = {}", mem::size_of::<Option<u8>>());
    println!("  size_of::<Option<Box<i32>>>() = {} (niche optimisation!)",
        mem::size_of::<Option<Box<i32>>>());
    println!("  align_of::<u64>()  = {}", mem::align_of::<u64>());

    // swap
    let mut a = 10;
    let mut b = 20;
    mem::swap(&mut a, &mut b);
    println!("  after swap: a={a}, b={b}");

    // replace — set a new value, get the old one back
    let mut s = String::from("hello");
    let old = mem::replace(&mut s, String::from("world"));
    println!("  replace: old='{old}', new='{s}'");

    // take — replace with Default, return old value
    let mut v = vec![1, 2, 3];
    let taken = mem::take(&mut v);
    println!("  take: taken={taken:?}, v={v:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 47. BINARYHEAP (priority queue)
// ─────────────────────────────────────────────────────────────────────────────

fn binary_heap_demo() {
    use std::collections::BinaryHeap;
    use std::cmp::Reverse;

    // Max-heap (default)
    let mut heap = BinaryHeap::from(vec![3, 1, 4, 1, 5, 9, 2, 6]);
    println!("  max-heap peek: {:?}", heap.peek());
    let mut sorted_desc = Vec::new();
    while let Some(val) = heap.pop() {
        sorted_desc.push(val);
    }
    println!("  popped order (desc): {sorted_desc:?}");

    // Min-heap using Reverse
    let mut min_heap: BinaryHeap<Reverse<i32>> = BinaryHeap::new();
    for &x in &[5, 2, 8, 1, 9] {
        min_heap.push(Reverse(x));
    }
    let mut sorted_asc = Vec::new();
    while let Some(Reverse(val)) = min_heap.pop() {
        sorted_asc.push(val);
    }
    println!("  min-heap order (asc): {sorted_asc:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 48. SATURATING / WRAPPING / CHECKED ARITHMETIC
// ─────────────────────────────────────────────────────────────────────────────

fn arithmetic_demo() {
    // Saturating — clamps at type bounds
    println!("  u8: 250_u8.saturating_add(20) = {}", 250_u8.saturating_add(20));
    println!("  i8: (-100_i8).saturating_sub(50) = {}", (-100_i8).saturating_sub(50));

    // Wrapping — wraps around like C
    println!("  u8: 250_u8.wrapping_add(20) = {}", 250_u8.wrapping_add(20));

    // Checked — returns None on overflow
    println!("  u8: 250_u8.checked_add(20) = {:?}", 250_u8.checked_add(20));
    println!("  u8: 200_u8.checked_add(20) = {:?}", 200_u8.checked_add(20));

    // Overflowing — returns (result, did_overflow)
    println!("  u8: 250_u8.overflowing_add(20) = {:?}", 250_u8.overflowing_add(20));

    // Power, log, integer division rounding
    println!("  2_u32.pow(10) = {}", 2_u32.pow(10));
    println!("  1000_u32.ilog10() = {}", 1000_u32.ilog10());
    println!("  7_u32.div_ceil(3) = {}", 7_u32.div_ceil(3));
}

// ─────────────────────────────────────────────────────────────────────────────
// 49. LET-ELSE & IF-LET CHAINS
// ─────────────────────────────────────────────────────────────────────────────

fn let_else_demo() {
    // let-else: destructure or diverge
    fn parse_pair(input: &str) -> String {
        let Some((left, right)) = input.split_once(':') else {
            return format!("  '{input}' — no colon found");
        };
        format!("  '{input}' → left='{left}', right='{right}'")
    }
    println!("{}", parse_pair("key:value"));
    println!("{}", parse_pair("nodelimiter"));

    // if-let with boolean guards
    let config: Option<(u16, bool)> = Some((443, true));
    if let Some((port, tls)) = config {
        println!("  if-let: port={port}, tls={tls}");
    }

    // while-let draining a Vec
    let mut stack = vec![10, 20, 30];
    let mut popped = Vec::new();
    while let Some(val) = stack.pop() {
        popped.push(val);
    }
    println!("  while-let popped: {popped:?}");

    // matches! macro
    let x = Some(42);
    println!("  matches!(Some(42), Some(40..=50)) = {}", matches!(x, Some(40..=50)));
    println!("  matches!(Some(42), Some(0..=10))  = {}", matches!(x, Some(0..=10)));
}

// ─────────────────────────────────────────────────────────────────────────────
// 50. STD::FMT FORMATTING TRICKS
// ─────────────────────────────────────────────────────────────────────────────

fn fmt_demo() {
    // Width and alignment
    println!("  |{:<15}| left",   "left");
    println!("  |{:>15}| right",  "right");
    println!("  |{:^15}| center", "center");
    println!("  |{:-^15}| fill",  "fill");

    // Number formatting
    println!("  binary:  {:08b}", 42);
    println!("  octal:   {:05o}", 42);
    println!("  hex:     {:04x}", 255);
    println!("  HEX:     {:04X}", 255);
    println!("  sci:     {:e}",   12345.6789);
    println!("  +sign:   {:+}",   42);

    // Precision
    println!("  pi 2dp:  {:.2}",  std::f64::consts::PI);
    println!("  pi 8dp:  {:.8}",  std::f64::consts::PI);
    println!("  trunc:   {:.5}",  "hello world");

    // Debug vs Display, pretty-print
    let v = vec![1, 2, 3];
    println!("  display: not available for Vec");
    println!("  debug:   {:?}", v);
    println!("  pretty:  {:#?}", v);

    // Named parameters
    println!("  {name} is {age}", name = "Alice", age = 30);

    // Dynamic width/precision
    let width = 20;
    let prec = 4;
    println!("  dynamic: {:>width$.prec$}", std::f64::consts::E);
}

// ─────────────────────────────────────────────────────────────────────────────
// 51. DEREF, INDEX & CUSTOM SMART POINTER
// ─────────────────────────────────────────────────────────────────────────────

use std::ops::{Deref, Index};

struct MyBox<T>(T);

impl<T> MyBox<T> {
    fn new(x: T) -> MyBox<T> { MyBox(x) }
}

impl<T> Deref for MyBox<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}

impl<T> Display for MyBox<T> where T: Display {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MyBox({})", self.0)
    }
}

struct Matrix {
    data: Vec<Vec<f64>>,
    rows: usize,
    cols: usize,
}

impl Matrix {
    fn new(rows: usize, cols: usize) -> Self {
        Matrix { data: vec![vec![0.0; cols]; rows], rows, cols }
    }
    fn set(&mut self, r: usize, c: usize, val: f64) { self.data[r][c] = val; }
}

impl Index<(usize, usize)> for Matrix {
    type Output = f64;
    fn index(&self, idx: (usize, usize)) -> &f64 { &self.data[idx.0][idx.1] }
}

impl Display for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for row in &self.data {
            let s: Vec<String> = row.iter().map(|v| format!("{v:6.1}")).collect();
            writeln!(f, "  [{}]", s.join(", "))?;
        }
        Ok(())
    }
}

fn deref_index_demo() {
    // Deref coercion
    let boxed = MyBox::new(String::from("hello"));
    // MyBox<String> → &String → &str via deref coercion chain
    fn greet(name: &str) { println!("  deref coercion: Hello, {name}!"); }
    greet(&boxed);
    println!("  {boxed}");

    // Custom Index
    let mut m = Matrix::new(2, 3);
    m.set(0, 1, 3.14);
    m.set(1, 2, 2.71);
    println!("  matrix[(0,1)] = {}", m[(0, 1)]);
    println!("  matrix:\n{m}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 52. STD::ARRAY::FROM_FN & ARRAY METHODS
// ─────────────────────────────────────────────────────────────────────────────

fn array_demo() {
    // array::from_fn — build array from index
    let squares: [i32; 8] = std::array::from_fn(|i| (i as i32 + 1).pow(2));
    println!("  from_fn squares: {squares:?}");

    // array .map()
    let doubled: [i32; 8] = squares.map(|x| x * 2);
    println!("  doubled: {doubled:?}");

    // array .each_ref(), .each_mut()
    let refs: [&i32; 8] = squares.each_ref();
    println!("  each_ref[0]: {}", refs[0]);

    // windows and chunks on slices
    let data = [1, 2, 3, 4, 5, 6, 7, 8];
    let wins: Vec<&[i32]> = data.windows(3).collect();
    println!("  windows(3): {wins:?}");

    let chunks: Vec<&[i32]> = data.chunks(3).collect();
    println!("  chunks(3): {chunks:?}");

    // split_at, rotate
    let mut arr = [1, 2, 3, 4, 5];
    let (left, right) = arr.split_at(3);
    println!("  split_at(3): left={left:?}, right={right:?}");
    arr.rotate_left(2);
    println!("  rotate_left(2): {arr:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 53. STD::IO::CURSOR — in-memory I/O
// ─────────────────────────────────────────────────────────────────────────────

fn cursor_demo() {
    use std::io::{Cursor, Read, Write, BufRead, Seek, SeekFrom};

    // Write to an in-memory buffer
    let mut cursor = Cursor::new(Vec::new());
    write!(cursor, "Hello, ").unwrap();
    write!(cursor, "Cursor!").unwrap();
    let buffer = cursor.into_inner();
    println!("  written: '{}'", String::from_utf8(buffer).unwrap());

    // Read from an in-memory buffer
    let data = b"line one\nline two\nline three";
    let cursor = Cursor::new(data);
    let lines: Vec<String> = cursor.lines().map(|l| l.unwrap()).collect();
    println!("  lines read: {lines:?}");

    // Seek
    let mut cursor = Cursor::new(b"abcdefghij");
    cursor.seek(SeekFrom::Start(5)).unwrap();
    let mut buf = [0u8; 3];
    cursor.read_exact(&mut buf).unwrap();
    println!("  seek(5)+read(3): '{}'", std::str::from_utf8(&buf).unwrap());
}

// ─────────────────────────────────────────────────────────────────────────────
// 54. STD::ENV — environment & args
// ─────────────────────────────────────────────────────────────────────────────

fn env_demo() {
    // Command-line args
    let args: Vec<String> = std::env::args().collect();
    println!("  argv[0]: {}", args[0]);
    println!("  argc: {}", args.len());

    // Environment variables
    println!("  HOME = {:?}", std::env::var("HOME").unwrap_or_default());
    println!("  NONEXISTENT = {:?}", std::env::var("NONEXISTENT"));

    // Current directory
    if let Ok(cwd) = std::env::current_dir() {
        println!("  cwd: {}", cwd.display());
    }

    // Temp dir
    println!("  temp_dir: {}", std::env::temp_dir().display());

    // Target arch and OS at compile time
    println!("  target_os: {}", std::env::consts::OS);
    println!("  target_arch: {}", std::env::consts::ARCH);
    println!("  exe_suffix: '{}'", std::env::consts::EXE_SUFFIX);
}

// ─────────────────────────────────────────────────────────────────────────────
// 55. PHANTOMDATA & ZERO-SIZED TYPES
// ─────────────────────────────────────────────────────────────────────────────

use std::marker::PhantomData;

struct Authenticated;
struct Guest;

struct Token<State> {
    user: String,
    _state: PhantomData<State>,
}

impl Token<Guest> {
    fn new(user: &str) -> Token<Guest> {
        Token { user: user.to_string(), _state: PhantomData }
    }
    fn authenticate(self, _password: &str) -> Token<Authenticated> {
        println!("  authenticating '{}'...", self.user);
        Token { user: self.user, _state: PhantomData }
    }
}

impl Token<Authenticated> {
    fn access_secret(&self) -> &str {
        "top-secret-data"
    }
}

fn phantom_demo() {
    println!("  size_of Token<Guest>: {} (zero-cost!)", std::mem::size_of::<PhantomData<Guest>>());

    let guest = Token::<Guest>::new("alice");
    // guest.access_secret(); // won't compile — type-state prevents it!
    let authed = guest.authenticate("password123");
    println!("  secret: {}", authed.access_secret());
}

// ─────────────────────────────────────────────────────────────────────────────
// 56. CUSTOM ORD & SORTING
// ─────────────────────────────────────────────────────────────────────────────

fn ordering_demo() {
    use std::cmp::Ordering;

    #[derive(Debug, Eq, PartialEq)]
    struct Task { priority: u8, name: String }

    impl PartialOrd for Task {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
    }
    impl Ord for Task {
        fn cmp(&self, other: &Self) -> Ordering {
            self.priority.cmp(&other.priority).reverse()  // higher priority first
                .then_with(|| self.name.cmp(&other.name)) // then alphabetical
        }
    }

    let mut tasks = vec![
        Task { priority: 2, name: "write docs".into() },
        Task { priority: 5, name: "fix bug".into() },
        Task { priority: 5, name: "add tests".into() },
        Task { priority: 1, name: "refactor".into() },
    ];
    tasks.sort();
    for t in &tasks {
        println!("  [p={}] {}", t.priority, t.name);
    }

    // sort_by_key, sort_unstable_by
    let mut nums = vec![3, -1, 4, -1, 5, -9, 2, 6];
    nums.sort_by_key(|&x: &i32| x.abs());
    println!("  sorted by abs: {nums:?}");

    // Ord::clamp
    println!("  15_i32.clamp(0, 10) = {}", 15_i32.clamp(0, 10));
    println!("  (-5_i32).clamp(0, 10) = {}", (-5_i32).clamp(0, 10));
}

// ─────────────────────────────────────────────────────────────────────────────
// 57. ITERATOR CONSTRUCTORS — from_fn, successors, repeat_with
// ─────────────────────────────────────────────────────────────────────────────

fn iter_constructors_demo() {
    // std::iter::from_fn — stateful closure
    let mut count = 0;
    let first_5: Vec<i32> = std::iter::from_fn(|| {
        count += 1;
        if count <= 5 { Some(count * count) } else { None }
    }).collect();
    println!("  from_fn (squares): {first_5:?}");

    // std::iter::successors — each element depends on previous
    let powers_of_2: Vec<u64> = std::iter::successors(Some(1_u64), |&prev| {
        prev.checked_mul(2)
    }).take(10).collect();
    println!("  successors (powers of 2): {powers_of_2:?}");

    // std::iter::repeat_with — lazy repeated computation
    let mut rng_state = 42_u64;
    let pseudo_random: Vec<u64> = std::iter::repeat_with(|| {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        rng_state >> 33
    }).take(5).collect();
    println!("  repeat_with (pseudo-random): {pseudo_random:?}");

    // std::iter::once and std::iter::empty
    let combined: Vec<i32> = std::iter::once(0)
        .chain(1..=3)
        .chain(std::iter::once(99))
        .collect();
    println!("  once + chain: {combined:?}");

    // scan — fold that yields intermediate values
    let running_sum: Vec<i32> = [1, 2, 3, 4, 5]
        .iter()
        .scan(0, |acc, &x| { *acc += x; Some(*acc) })
        .collect();
    println!("  scan (running sum): {running_sum:?}");

    // peekable
    let mut iter = [1, 2, 3].iter().peekable();
    let peeked = iter.peek().copied();
    let next = iter.next();
    println!("  peekable — peek: {peeked:?}, next: {next:?}");

    // partition
    let (evens, odds): (Vec<i32>, Vec<i32>) = (1..=10).partition(|x| x % 2 == 0);
    println!("  partition evens: {evens:?}");
    println!("  partition odds:  {odds:?}");

    // unzip
    let (keys, vals): (Vec<&str>, Vec<i32>) = [("a", 1), ("b", 2), ("c", 3)]
        .iter().copied().unzip();
    println!("  unzip keys: {keys:?}, vals: {vals:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 58. STD::PROCESS::COMMAND
// ─────────────────────────────────────────────────────────────────────────────

fn process_demo() {
    use std::process::Command;

    // Run a command and capture output
    match Command::new("echo").arg("hello from subprocess").output() {
        Ok(output) => {
            println!("  status: {}", output.status);
            println!("  stdout: {}", String::from_utf8_lossy(&output.stdout).trim());
        }
        Err(e) => println!("  failed to run echo: {e}"),
    }

    // Check if a command exists by running it
    let has_rustc = Command::new("rustc").arg("--version").output().is_ok();
    println!("  rustc available: {has_rustc}");

    // Environment variable override
    match Command::new("sh")
        .arg("-c")
        .arg("echo $MY_VAR")
        .env("MY_VAR", "injected_value")
        .output()
    {
        Ok(output) => println!("  env override: {}", String::from_utf8_lossy(&output.stdout).trim()),
        Err(e) => println!("  failed: {e}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 59. ALL STANDARD MACROS
// ─────────────────────────────────────────────────────────────────────────────

fn all_macros_demo() {
    // ── Output macros ───────────────────────────────────────────────────────
    // print! — stdout without newline
    print!("  print! no newline → ");
    // println! — stdout with newline (used everywhere)
    println!("followed by println!");
    // eprint! — stderr without newline
    eprint!("");
    // eprintln! — stderr with newline
    eprintln!("  eprintln! goes to stderr");

    // ── dbg! — debug print with file:line and returns the value ─────────
    let val = dbg!(2 + 3); // prints to stderr: [main.rs:LINE] 2 + 3 = 5
    println!("  dbg! returned: {val}");

    // ── format! — format string into String (already used throughout) ───
    let s = format!("{:>10} padded", "right");
    println!("  format!: '{s}'");

    // ── vec! — vector literal (already used throughout) ─────────────────
    let v = vec![0; 5]; // five zeros
    println!("  vec![0; 5]: {v:?}");

    // ── write! / writeln! — write into any fmt::Write or io::Write ──────
    use std::fmt::Write as FmtWrite;
    let mut buf = String::new();
    write!(buf, "hello").unwrap();
    writeln!(buf, " world").unwrap();
    println!("  write!/writeln! to String: '{}'", buf.trim());

    // ── assert!, assert_eq!, assert_ne! — runtime assertions ────────────
    assert!(1 + 1 == 2);
    assert_eq!(4 * 5, 20);
    assert_ne!("hello", "world");
    println!("  assert!, assert_eq!, assert_ne! all passed");

    // ── debug_assert!, debug_assert_eq!, debug_assert_ne! ───────────────
    // Only checked in debug builds (removed in release)
    debug_assert!(true, "only in debug");
    debug_assert_eq!(10, 10);
    debug_assert_ne!(1, 2);
    println!("  debug_assert!, debug_assert_eq!, debug_assert_ne! passed (debug build)");

    // ── panic! — unrecoverable error (caught with catch_unwind) ─────────
    let caught = std::panic::catch_unwind(|| {
        panic!("intentional panic for demo");
    });
    println!("  panic! caught: {}", caught.is_err());

    // ── todo! — marks unfinished code ───────────────────────────────────
    let caught = std::panic::catch_unwind(|| {
        #[allow(unreachable_code)]
        { todo!("implement this later"); }
    });
    println!("  todo! caught: {}", caught.is_err());

    // ── unimplemented! — marks intentionally unimplemented code ─────────
    let caught = std::panic::catch_unwind(|| {
        #[allow(unreachable_code)]
        { unimplemented!("not yet done"); }
    });
    println!("  unimplemented! caught: {}", caught.is_err());

    // ── unreachable! — marks code that should never execute ─────────────
    let x = 1;
    let desc = match x {
        1 => "one",
        2 => "two",
        _ => unreachable!("x is always 1 or 2 in this demo"),
    };
    println!("  unreachable! (not triggered): desc={desc}");

    // ── cfg! — evaluate cfg conditions at runtime ───────────────────────
    println!("  cfg!(target_os = \"linux\"): {}", cfg!(target_os = "linux"));
    println!("  cfg!(debug_assertions):     {}", cfg!(debug_assertions));
    println!("  cfg!(target_pointer_width = \"64\"): {}", cfg!(target_pointer_width = "64"));

    // ── file!, line!, column! — source location ─────────────────────────
    println!("  file!: {}", file!());
    println!("  line!: {}", line!());
    println!("  column!: {}", column!());

    // ── module_path! — full module path ─────────────────────────────────
    println!("  module_path!: {}", module_path!());

    // ── stringify! — turn expression into string literal ────────────────
    println!("  stringify!(1 + 2 * 3): {}", stringify!(1 + 2 * 3));
    println!("  stringify!(Vec<i32>):  {}", stringify!(Vec<i32>));

    // ── concat! — concatenate literals at compile time ──────────────────
    let s: &str = concat!("hello", ' ', "world", ' ', 42, ' ', true);
    println!("  concat!: {s}");

    // ── env! — compile-time environment variable (must exist) ───────────
    // CARGO_PKG_NAME may not exist when compiled with rustc directly
    // So we use a universally available one or fall back
    println!("  env!(\"PATH\") starts with: {}...", &env!("PATH")[..20]);

    // ── option_env! — compile-time env that may not exist ───────────────
    let cargo: Option<&str> = option_env!("CARGO_PKG_NAME");
    println!("  option_env!(\"CARGO_PKG_NAME\"): {:?}", cargo);
    let missing: Option<&str> = option_env!("DEFINITELY_NOT_SET_XYZ_12345");
    println!("  option_env!(\"DEFINITELY_NOT_SET_...\"): {:?}", missing);

    // ── include_str! / include_bytes! — embed file contents at compile time
    // Requires a file to exist; we create a small demo
    // println!("  include_str!: {}", include_str!("main.rs")[..30]);
    println!("  include_str!/include_bytes!: (embeds file at compile time)");

    // ── matches! — pattern match returning bool (already used in §49) ───
    let val = Some(42);
    println!("  matches!(Some(42), Some(1..=100)): {}", matches!(val, Some(1..=100)));
    println!("  matches!(Some(42), None):          {}", matches!(val, None));

    // ── thread_local! — thread-local storage (already used in §33) ──────
    thread_local! {
        static DEMO_TLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }
    DEMO_TLS.with(|c| { c.set(99); println!("  thread_local! value: {}", c.get()); });

    // ── Custom macros (already shown in §13): macro_rules! ──────────────
    println!("  map!/max! (custom macro_rules!): already demonstrated in §13");

    // ── Summary ─────────────────────────────────────────────────────────
    println!("  ---");
    println!("  Macros demonstrated: print!, println!, eprint!, eprintln!,");
    println!("    dbg!, format!, vec!, write!, writeln!, assert!, assert_eq!,");
    println!("    assert_ne!, debug_assert!, debug_assert_eq!, debug_assert_ne!,");
    println!("    panic!, todo!, unimplemented!, unreachable!, cfg!, file!,");
    println!("    line!, column!, module_path!, stringify!, concat!, env!,");
    println!("    option_env!, matches!, thread_local!, macro_rules!");
}

// ─────────────────────────────────────────────────────────────────────────────
// 61. SERDE & SERDE_JSON — serialization / deserialization
// ─────────────────────────────────────────────────────────────────────────────

fn serde_demo() {
    use serde::{Serialize, Deserialize};

    // ── Derive Serialize / Deserialize ──────────────────────────────────
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Config {
        host: String,
        port: u16,
        tags: Vec<String>,
        debug: bool,
    }

    let cfg = Config {
        host: "localhost".into(),
        port: 8080,
        tags: vec!["web".into(), "api".into()],
        debug: true,
    };

    // to_string (serialize)
    let json = serde_json::to_string(&cfg).unwrap();
    println!("  to_string:        {json}");

    // to_string_pretty
    let pretty = serde_json::to_string_pretty(&cfg).unwrap();
    println!("  to_string_pretty:\n{pretty}");

    // from_str (deserialize)
    let back: Config = serde_json::from_str(&json).unwrap();
    println!("  from_str:         {back:?}");
    assert_eq!(cfg, back);
    println!("  round-trip:       OK (cfg == back)");

    // ── serde_json::json! macro — build JSON inline ─────────────────────
    let val = serde_json::json!({
        "name": "Alice",
        "age": 30,
        "scores": [95, 87, 100],
        "address": {
            "city": "Stockholm",
            "zip": "114 55"
        },
        "active": true,
        "nickname": null
    });
    println!("  json! macro:      {val}");
    println!("    [\"name\"]:       {}", val["name"]);
    println!("    [\"scores\"][1]:  {}", val["scores"][1]);
    println!("    [\"address\"]:    {}", val["address"]);

    // ── serde_json::Value — dynamic JSON ────────────────────────────────
    let raw = r#"{"x": 1, "y": [2, 3], "z": {"nested": true}}"#;
    let parsed: serde_json::Value = serde_json::from_str(raw).unwrap();
    println!("  Value parse:      {parsed}");
    println!("    is_object:      {}", parsed.is_object());
    println!("    [\"y\"][0]:       {}", parsed["y"][0]);
    println!("    [\"z\"][\"nested\"]: {}", parsed["z"]["nested"]);

    // ── Rename, default, skip, flatten ───────────────────────────────────
    #[derive(Debug, Serialize, Deserialize)]
    struct ApiResponse {
        #[serde(rename = "statusCode")]
        status_code: u16,

        #[serde(default)]
        message: String,

        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,

        #[serde(flatten)]
        extra: std::collections::HashMap<String, serde_json::Value>,
    }

    let resp = ApiResponse {
        status_code: 200,
        message: "OK".into(),
        error: None,
        extra: [("request_id".into(), serde_json::json!("abc-123"))].into_iter().collect(),
    };
    let j = serde_json::to_string(&resp).unwrap();
    println!("  rename+skip+flatten: {j}");

    // Deserialize with missing optional + default
    let minimal = r#"{"statusCode": 404}"#;
    let resp2: ApiResponse = serde_json::from_str(minimal).unwrap();
    println!("  from minimal JSON:   status={}, message='{}'", resp2.status_code, resp2.message);

    // ── Enum serialization ──────────────────────────────────────────────
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "type", content = "data")]
    enum Event {
        Click { x: i32, y: i32 },
        KeyPress(char),
        Resize { width: u32, height: u32 },
    }

    let events = vec![
        Event::Click { x: 10, y: 20 },
        Event::KeyPress('a'),
        Event::Resize { width: 1920, height: 1080 },
    ];
    for e in &events {
        println!("  enum ser:   {}", serde_json::to_string(e).unwrap());
    }

    // Round-trip an enum
    let json = serde_json::to_string(&events[0]).unwrap();
    let back: Event = serde_json::from_str(&json).unwrap();
    println!("  enum deser: {back:?}");

    // ── from_value / to_value — convert between Value and typed ─────────
    let val = serde_json::json!({"host": "example.com", "port": 443, "tags": [], "debug": false});
    let cfg2: Config = serde_json::from_value(val.clone()).unwrap();
    println!("  from_value: {cfg2:?}");
    let back_val = serde_json::to_value(&cfg2).unwrap();
    println!("  to_value:   {back_val}");

    // ── Streaming: to_writer / from_reader ──────────────────────────────
    let mut buf: Vec<u8> = Vec::new();
    serde_json::to_writer(&mut buf, &cfg).unwrap();
    let from_reader: Config = serde_json::from_reader(buf.as_slice()).unwrap();
    println!("  to_writer → from_reader: {from_reader:?}");

    // ── Summary ─────────────────────────────────────────────────────────
    println!("  ---");
    println!("  serde:      Serialize, Deserialize, #[serde(rename, default,");
    println!("              skip_serializing_if, flatten, tag, content)]");
    println!("  serde_json: to_string, to_string_pretty, from_str, json!,");
    println!("              Value, from_value, to_value, to_writer, from_reader");
}

// ─────────────────────────────────────────────────────────────────────────────
// 62. ALL CHANNEL TYPES (std + tokio)
// ─────────────────────────────────────────────────────────────────────────────

async fn all_channels_demo() {
    // ── 1. std::sync::mpsc::channel — unbounded, multi-producer ─────────
    println!("  ── std::sync::mpsc::channel (unbounded) ──");
    {
        let (tx, rx) = mpsc::channel();
        let tx2 = tx.clone(); // clone for second producer
        thread::spawn(move || { tx.send("std-mpsc-1").unwrap(); });
        thread::spawn(move || { tx2.send("std-mpsc-2").unwrap(); });
        let mut msgs: Vec<&str> = Vec::new();
        for _ in 0..2 { msgs.push(rx.recv().unwrap()); }
        msgs.sort();
        println!("    recv: {msgs:?}");

        // Iterate over channel (consumes until all senders drop)
        let (tx, rx) = mpsc::channel();
        for i in 0..3 { tx.send(i * 10).unwrap(); }
        drop(tx); // close the channel
        let collected: Vec<i32> = rx.iter().collect();
        println!("    iter (after drop tx): {collected:?}");

        // try_recv — non-blocking
        let (tx, rx) = mpsc::channel::<i32>();
        println!("    try_recv (empty): {:?}", rx.try_recv());
        tx.send(42).unwrap();
        println!("    try_recv (has value): {:?}", rx.try_recv());
    }

    // ── 2. std::sync::mpsc::sync_channel — bounded, backpressure ───────
    println!("  ── std::sync::mpsc::sync_channel (bounded) ──");
    {
        let (tx, rx) = mpsc::sync_channel::<i32>(2); // buffer size 2
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        // tx.send(3) would block here — buffer full!
        println!("    try_send(3) on full: {:?}", tx.try_send(3));
        println!("    recv: {}, {}", rx.recv().unwrap(), rx.recv().unwrap());
        // Now there's room
        tx.send(3).unwrap();
        println!("    recv after drain: {}", rx.recv().unwrap());

        // Zero-capacity (rendezvous) channel — sender blocks until receiver reads
        let (tx, rx) = mpsc::sync_channel::<&str>(0);
        thread::spawn(move || {
            tx.send("rendezvous").unwrap(); // blocks until recv
        });
        println!("    rendezvous: {}", rx.recv().unwrap());
    }

    // ── 3. tokio::sync::mpsc — async bounded multi-producer ────────────
    println!("  ── tokio::sync::mpsc (async, bounded) ──");
    {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(8);
        let tx2 = tx.clone();
        tokio::spawn(async move { tx.send("tokio-mpsc-1".into()).await.unwrap(); });
        tokio::spawn(async move { tx2.send("tokio-mpsc-2".into()).await.unwrap(); });
        let mut msgs = Vec::new();
        for _ in 0..2 { msgs.push(rx.recv().await.unwrap()); }
        msgs.sort();
        println!("    recv: {msgs:?}");
    }

    // ── 4. tokio::sync::mpsc::unbounded_channel — async unbounded ──────
    println!("  ── tokio::sync::mpsc::unbounded_channel ──");
    {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        for i in 0..5 { tx.send(i * 100).unwrap(); } // send is sync — never blocks
        drop(tx);
        let mut collected = Vec::new();
        while let Some(val) = rx.recv().await {
            collected.push(val);
        }
        println!("    collected: {collected:?}");
    }

    // ── 5. tokio::sync::oneshot — single value, single producer/consumer
    println!("  ── tokio::sync::oneshot ──");
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            tx.send("oneshot-value".into()).unwrap();
        });
        println!("    recv: {}", rx.await.unwrap());

        // Dropped sender → RecvError
        let (tx, rx) = tokio::sync::oneshot::channel::<i32>();
        drop(tx);
        println!("    dropped tx: {:?}", rx.await);
    }

    // ── 6. tokio::sync::broadcast — multi-producer, multi-consumer ─────
    println!("  ── tokio::sync::broadcast (multi-consumer) ──");
    {
        let (tx, _) = tokio::sync::broadcast::channel::<String>(16);
        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();

        tx.send("broadcast-msg-1".into()).unwrap();
        tx.send("broadcast-msg-2".into()).unwrap();

        // Both receivers get ALL messages
        println!("    rx1: {}, {}", rx1.recv().await.unwrap(), rx1.recv().await.unwrap());
        println!("    rx2: {}, {}", rx2.recv().await.unwrap(), rx2.recv().await.unwrap());

        // Late subscriber misses past messages
        let mut rx3 = tx.subscribe();
        tx.send("broadcast-msg-3".into()).unwrap();
        println!("    rx3 (late): {}", rx3.recv().await.unwrap());
    }

    // ── 7. tokio::sync::watch — single value, latest-only ──────────────
    println!("  ── tokio::sync::watch (latest value only) ──");
    {
        let (tx, mut rx) = tokio::sync::watch::channel("initial");
        println!("    initial: {}", *rx.borrow());

        tx.send("update-1").unwrap();
        tx.send("update-2").unwrap();
        tx.send("update-3").unwrap();
        // Receiver only sees the LATEST value, not all intermediate
        rx.changed().await.unwrap();
        println!("    after 3 sends: {}", *rx.borrow());

        // Multiple receivers all see same latest value
        let rx2 = tx.subscribe();
        println!("    new subscriber sees: {}", *rx2.borrow());

        // send_if_modified — only notify if value actually changed
        tx.send_if_modified(|val| {
            if *val != "update-3" { *val = "modified"; true }
            else { false } // no change, no notification
        });
        println!("    after send_if_modified (no change): {}", *rx.borrow());
    }

    // ── 8. tokio::sync::Notify — signal without data ───────────────────
    println!("  ── tokio::sync::Notify (signal without data) ──");
    {
        let notify = std::sync::Arc::new(tokio::sync::Notify::new());
        let notify2 = notify.clone();

        let handle = tokio::spawn(async move {
            notify2.notified().await;
            "woke up!"
        });

        // Small yield to let spawned task register the waiter
        tokio::task::yield_now().await;
        notify.notify_one();
        println!("    {}", handle.await.unwrap());
    }

    // ── 9. tokio::sync::Semaphore — bounded concurrency ────────────────
    println!("  ── tokio::sync::Semaphore (bounded concurrency) ──");
    {
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(2)); // max 2 concurrent
        let mut handles = Vec::new();
        for i in 0..4 {
            let sem = sem.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                // Only 2 tasks run here at a time
                format!("task-{i}")
            }));
        }
        let mut results = Vec::new();
        for h in handles { results.push(h.await.unwrap()); }
        results.sort();
        println!("    completed: {results:?}");
        println!("    permits available: {}", sem.available_permits());
    }

    // ── 10. tokio::sync::RwLock — async reader-writer lock ──────────────
    println!("  ── tokio::sync::RwLock ──");
    {
        let data = std::sync::Arc::new(tokio::sync::RwLock::new(vec![1, 2, 3]));

        // Multiple concurrent readers
        let d = data.clone();
        let r1 = tokio::spawn(async move { d.read().await.len() });
        let d = data.clone();
        let r2 = tokio::spawn(async move { d.read().await.iter().sum::<i32>() });

        println!("    readers: len={}, sum={}", r1.await.unwrap(), r2.await.unwrap());

        // Exclusive writer
        data.write().await.push(4);
        println!("    after write: {:?}", *data.read().await);
    }

    // ── 11. tokio::sync::Barrier — sync point for N tasks ───────────────
    println!("  ── tokio::sync::Barrier ──");
    {
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for i in 0..3 {
            let b = barrier.clone();
            handles.push(tokio::spawn(async move {
                // All 3 tasks must arrive before any proceed
                let result = b.wait().await;
                (i, result.is_leader()) // exactly one is the "leader"
            }));
        }
        let mut results = Vec::new();
        for h in handles { results.push(h.await.unwrap()); }
        results.sort();
        let leaders: Vec<_> = results.iter().filter(|(_, is_leader)| *is_leader).collect();
        println!("    all 3 arrived, leaders: {}", leaders.len());
        println!("    results: {results:?}");
    }

    // ── Summary ─────────────────────────────────────────────────────────
    println!("  ---");
    println!("  std:   mpsc::channel, mpsc::sync_channel (bounded/rendezvous)");
    println!("  tokio: mpsc, mpsc::unbounded, oneshot, broadcast, watch,");
    println!("         Notify, Semaphore, RwLock, Barrier");
}

fn section(title: &str) {
    println!("\n{}", "─".repeat(60));
    println!("  {}", title);
    println!("{}", "─".repeat(60));
}
