#!/usr/bin/env python3
"""
╔══════════════════════════════════════════════════════════════════════════════╗
║              COMPREHENSIVE PYTHON FEATURES SHOWCASE                         ║
║              Every major Python feature in one file                          ║
╚══════════════════════════════════════════════════════════════════════════════╝

This file demonstrates virtually every Python language feature, organized
into clearly labeled sections. Run it to see output from each section.

Python version: 3.10+ recommended (for match/case, union types, etc.)
"""

from __future__ import annotations
import sys

print(f"Python {sys.version}\n{'=' * 70}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 1: BASIC DATA TYPES & VARIABLES                                ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§1 — BASIC DATA TYPES & VARIABLES")

# Integers (arbitrary precision)
big_int = 10**100
hex_val, oct_val, bin_val = 0xFF, 0o77, 0b1010
underscored = 1_000_000_000

# Floats & complex numbers
pi = 3.14159
scientific = 6.022e23
complex_num = 3 + 4j
print(f"  Complex: {complex_num}, magnitude: {abs(complex_num)}")

# Strings
single = "hello"
double = "world"
multiline = """This is a
multiline string."""
raw = r"No \n escape here"
f_string = f"pi ≈ {pi:.2f}"
byte_str = b"bytes literal"
print(f"  f-string: {f_string}, raw: {raw}")

# Boolean & None
flag = True
nothing = None
print(f"  bool is int subclass: {isinstance(True, int)}, True + True = {True + True}")

# Type checking
print(f"  type(42)={type(42).__name__}, type('hi')={type('hi').__name__}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 2: COLLECTIONS                                                  ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§2 — COLLECTIONS")

# Lists (mutable, ordered)
fruits = ["apple", "banana", "cherry"]
fruits.append("date")
fruits.insert(1, "avocado")
fruits.extend(["elderberry", "fig"])
sliced = fruits[1:4]
reversed_fruits = fruits[::-1]
print(f"  List slice: {sliced}")

# Tuples (immutable, ordered)
point = (3, 4)
single_element_tuple = (42,)
nested_tuple = ((1, 2), (3, 4))
x, y = point  # unpacking
print(f"  Tuple unpacking: x={x}, y={y}")

# Named tuples
from collections import namedtuple

Color = namedtuple("Color", ["red", "green", "blue"])
cyan = Color(0, 255, 255)
print(f"  NamedTuple: {cyan}, r={cyan.red}")

# Sets (unordered, unique)
primes = {2, 3, 5, 7, 11}
evens = {2, 4, 6, 8, 10}
print(f"  Set ops: union={primes | evens}, intersection={primes & evens}")
print(f"  Difference: {primes - evens}, symmetric_diff: {primes ^ evens}")
frozen = frozenset([1, 2, 3])  # immutable set

# Dictionaries (ordered since 3.7)
person = {"name": "Alice", "age": 30, "hobbies": ["reading", "coding"]}
person["email"] = "alice@example.com"
person |= {"city": "Stockholm"}  # merge operator (3.9+)
print(f"  Dict: {person['name']} from {person['city']}")
print(f"  Keys: {list(person.keys())}")

# DefaultDict, Counter, OrderedDict, ChainMap, deque
from collections import defaultdict, Counter, OrderedDict, ChainMap, deque

word_count = Counter("abracadabra")
print(f"  Counter: {word_count.most_common(3)}")

dd = defaultdict(list)
dd["fruits"].append("apple")
dd["fruits"].append("banana")
print(f"  DefaultDict: {dict(dd)}")

dq = deque([1, 2, 3], maxlen=5)
dq.appendleft(0)
dq.rotate(1)
print(f"  Deque: {list(dq)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 3: OPERATORS                                                    ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§3 — OPERATORS")

# Arithmetic
print(f"  floor div: 7//2={7 // 2}, mod: 7%2={7 % 2}, power: 2**10={2**10}")

# Walrus operator (:=)
data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
if (n := len(data)) > 5:
    print(f"  Walrus: list has {n} elements (>5)")

# Ternary / conditional expression
status = "even" if 42 % 2 == 0 else "odd"
print(f"  Ternary: 42 is {status}")

# Chained comparisons
x = 5
print(f"  Chained: 1 < {x} < 10 → {1 < x < 10}")

# Identity vs equality
a = [1, 2, 3]
b = a
c = [1, 2, 3]
print(f"  a is b: {a is b}, a is c: {a is c}, a == c: {a == c}")

# Bitwise
print(f"  Bitwise: 0b1100 & 0b1010 = {0b1100 & 0b1010:#06b}")
print(f"  Bitwise: 0b1100 | 0b1010 = {0b1100 | 0b1010:#06b}")
print(f"  Bitwise: 0b1100 ^ 0b1010 = {0b1100 ^ 0b1010:#06b}")
print(f"  Bitwise: ~5 = {~5}, 1 << 4 = {1 << 4}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 4: CONTROL FLOW                                                 ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§4 — CONTROL FLOW")

# if/elif/else
score = 85
grade = "A" if score >= 90 else "B" if score >= 80 else "C" if score >= 70 else "F"
print(f"  Grade for {score}: {grade}")

# for loops with else
for i in range(5):
    if i == 10:
        break
else:
    print("  for/else: loop completed without break")

# while with else
count = 0
while count < 3:
    count += 1
else:
    print(f"  while/else: finished at count={count}")


# match/case (structural pattern matching, 3.10+)
def classify(value):
    match value:
        case 0:
            return "zero"
        case int(n) if n > 0:
            return f"positive int: {n}"
        case int(n):
            return f"negative int: {n}"
        case float():
            return "float"
        case str() as s if len(s) > 5:
            return f"long string: '{s}'"
        case str() as s:
            return f"short string: '{s}'"
        case [x, y]:
            return f"2-element list: [{x}, {y}]"
        case {"name": name, "age": age}:
            return f"person dict: {name}, age {age}"
        case _:
            return "something else"


for val in [0, 42, -7, 3.14, "hi", "hello world", [1, 2], {"name": "Bob", "age": 25}]:
    print(f"  match {val!r:>25} → {classify(val)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 5: FUNCTIONS                                                    ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§5 — FUNCTIONS")


# Basic function with type hints
def greet(name: str, greeting: str = "Hello") -> str:
    """Greet someone (docstring)."""
    return f"{greeting}, {name}!"


print(f"  {greet('World')}")
print(f"  {greet('Python', greeting='Hej')}")


# *args and **kwargs
def variadic(*args: int, **kwargs: str) -> None:
    print(f"  args={args}, kwargs={kwargs}")


variadic(1, 2, 3, language="Python", version="3.12")


# Positional-only (/) and keyword-only (*) parameters
def strict(pos_only: int, /, normal: int, *, kw_only: int) -> tuple:
    return (pos_only, normal, kw_only)


print(f"  Strict params: {strict(1, 2, kw_only=3)}")

# Lambda
square = lambda x: x**2
print(f"  Lambda: square(7) = {square(7)}")

# Higher-order functions
from functools import reduce

nums = [1, 2, 3, 4, 5]
print(f"  map:    {list(map(lambda x: x * 2, nums))}")
print(f"  filter: {list(filter(lambda x: x % 2 == 0, nums))}")
print(f"  reduce: {reduce(lambda a, b: a + b, nums)}")


# Closures
def make_multiplier(factor: int):
    def multiply(x: int) -> int:
        return x * factor

    return multiply


triple = make_multiplier(3)
print(f"  Closure: triple(7) = {triple(7)}")


# Recursion
def factorial(n: int) -> int:
    return 1 if n <= 1 else n * factorial(n - 1)


print(f"  Recursion: 10! = {factorial(10)}")

# Function annotations inspection
print(f"  Annotations: {greet.__annotations__}")
print(f"  Docstring: {greet.__doc__}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 6: COMPREHENSIONS & GENERATOR EXPRESSIONS                      ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§6 — COMPREHENSIONS & GENERATORS")

# List comprehension
squares = [x**2 for x in range(10)]
print(f"  List comp: {squares}")

# Nested list comprehension
matrix = [[i * 3 + j for j in range(3)] for i in range(3)]
flat = [x for row in matrix for x in row]
print(f"  Matrix: {matrix}")
print(f"  Flattened: {flat}")

# Filtered comprehension
evens = [x for x in range(20) if x % 2 == 0]
print(f"  Filtered: {evens}")

# Dict comprehension
word_lengths = {w: len(w) for w in ["hello", "world", "python"]}
print(f"  Dict comp: {word_lengths}")

# Set comprehension
unique_lengths = {len(w) for w in ["hi", "hey", "hello", "yo"]}
print(f"  Set comp: {unique_lengths}")

# Generator expression (lazy)
gen = (x**2 for x in range(1_000_000))
print(f"  Generator (first 5): {[next(gen) for _ in range(5)]}")


# Generator function with yield
def fibonacci(limit: int):
    a, b = 0, 1
    while a < limit:
        yield a
        a, b = b, a + b


print(f"  Fibonacci: {list(fibonacci(100))}")


# yield from (delegation)
def chain_generators(*iterables):
    for it in iterables:
        yield from it


print(f"  yield from: {list(chain_generators([1, 2], [3, 4], [5, 6]))}")


# Send values to generator
def accumulator():
    total = 0
    while True:
        value = yield total
        if value is None:
            break
        total += value


gen = accumulator()
next(gen)  # prime the generator
print(f"  send to gen: {gen.send(10)}, {gen.send(20)}, {gen.send(5)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 7: DECORATORS                                                   ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§7 — DECORATORS")

import functools
import time


# Simple decorator
def timer(func):
    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        start = time.perf_counter()
        result = func(*args, **kwargs)
        elapsed = time.perf_counter() - start
        print(f"  @timer: {func.__name__} took {elapsed:.6f}s")
        return result

    return wrapper


# Decorator with arguments
def repeat(n: int):
    def decorator(func):
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            results = [func(*args, **kwargs) for _ in range(n)]
            return results

        return wrapper

    return decorator


# Stacking decorators
@timer
@repeat(3)
def say_hi(name: str) -> str:
    return f"Hi {name}!"


result = say_hi("Python")
print(f"  Stacked result: {result}")


# Class-based decorator
class CountCalls:
    def __init__(self, func):
        functools.update_wrapper(self, func)
        self.func = func
        self.count = 0

    def __call__(self, *args, **kwargs):
        self.count += 1
        return self.func(*args, **kwargs)


@CountCalls
def add(a, b):
    return a + b


add(1, 2)
add(3, 4)
add(5, 6)
print(f"  Class decorator: add called {add.count} times")


# functools.cache (memoization)
@functools.cache
def fib_cached(n: int) -> int:
    return n if n < 2 else fib_cached(n - 1) + fib_cached(n - 2)


print(f"  Cached fib(30) = {fib_cached(30)}")


# functools.lru_cache
@functools.lru_cache(maxsize=128)
def expensive(x: int) -> int:
    return x**x


expensive(10)
expensive(10)
print(f"  LRU cache info: {expensive.cache_info()}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 8: CLASSES & OOP                                                ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§8 — CLASSES & OOP")


# Full-featured class
class Animal:
    """A base class demonstrating many OOP features."""

    species_count: int = 0  # class variable

    def __init__(self, name: str, sound: str = "..."):
        self.name = name  # instance variable
        self._sound = sound  # convention: "protected"
        self.__id = id(self)  # name-mangled "private"
        Animal.species_count += 1

    # Properties (getter/setter/deleter)
    @property
    def sound(self) -> str:
        return self._sound

    @sound.setter
    def sound(self, value: str):
        self._sound = value.upper()

    # Instance method
    def speak(self) -> str:
        return f"{self.name} says {self._sound}"

    # Class method
    @classmethod
    def get_count(cls) -> int:
        return cls.species_count

    # Static method
    @staticmethod
    def is_animal(obj) -> bool:
        return isinstance(obj, Animal)

    # Dunder methods
    def __repr__(self) -> str:
        return f"Animal({self.name!r}, {self._sound!r})"

    def __str__(self) -> str:
        return f"{self.name} the animal"

    def __eq__(self, other) -> bool:
        return isinstance(other, Animal) and self.name == other.name

    def __hash__(self) -> int:
        return hash(self.name)

    def __len__(self) -> int:
        return len(self.name)

    def __contains__(self, item: str) -> bool:
        return item in self.name

    def __call__(self) -> str:
        return self.speak()


# Inheritance
class Dog(Animal):
    def __init__(self, name: str, breed: str):
        super().__init__(name, "Woof")
        self.breed = breed

    def speak(self) -> str:  # method override
        return f"{self.name} ({self.breed}) says {self._sound}!"

    def fetch(self, item: str) -> str:
        return f"{self.name} fetches the {item}"


# Multiple inheritance & MRO
class Pet:
    def __init__(self, owner: str = "Unknown"):
        self.owner = owner

    def who_owns(self) -> str:
        return f"Owned by {self.owner}"


class PetDog(Dog, Pet):
    def __init__(self, name: str, breed: str, owner: str):
        Dog.__init__(self, name, breed)
        Pet.__init__(self, owner)


rex = PetDog("Rex", "Labrador", "Alice")
print(f"  {rex.speak()}")
print(f"  {rex.who_owns()}")
print(f"  {rex.fetch('ball')}")
print(f"  MRO: {[c.__name__ for c in PetDog.__mro__]}")

# Property and dunder demos
cat = Animal("Whiskers", "Meow")
cat.sound = "purr"  # setter converts to uppercase
print(f"  Property setter: {cat.sound}")
print(f"  repr: {cat!r}")
print(f"  str: {cat}")
print(f"  len: {len(cat)}")
print(f"  contains 'isk': {'isk' in cat}")
print(f"  callable: {cat()}")
print(f"  Count: {Animal.get_count()} animals created")


# __slots__
class Optimized:
    __slots__ = ("x", "y")

    def __init__(self, x, y):
        self.x, self.y = x, y


opt = Optimized(1, 2)
print(f"  __slots__: {opt.x}, {opt.y}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 9: ABSTRACT CLASSES, PROTOCOLS & INTERFACES                     ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§9 — ABSTRACT CLASSES & PROTOCOLS")

from abc import ABC, abstractmethod
from typing import Protocol, runtime_checkable


# Abstract Base Class
class Shape(ABC):
    @abstractmethod
    def area(self) -> float: ...

    @abstractmethod
    def perimeter(self) -> float: ...

    def describe(self) -> str:
        return f"{self.__class__.__name__}: area={self.area():.2f}"


class Circle(Shape):
    def __init__(self, radius: float):
        self.radius = radius

    def area(self) -> float:
        return 3.14159 * self.radius**2

    def perimeter(self) -> float:
        return 2 * 3.14159 * self.radius


c = Circle(5)
print(f"  ABC: {c.describe()}, perimeter={c.perimeter():.2f}")


# Protocol (structural subtyping / duck typing)
@runtime_checkable
class Drawable(Protocol):
    def draw(self) -> str: ...


class Canvas:
    def draw(self) -> str:
        return "Drawing on canvas"


class NotDrawable:
    pass


print(f"  Protocol check Canvas: {isinstance(Canvas(), Drawable)}")
print(f"  Protocol check NotDrawable: {isinstance(NotDrawable(), Drawable)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 10: DATACLASSES, ENUMS, TYPING                                 ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§10 — DATACLASSES, ENUMS, TYPING")

from dataclasses import dataclass, field, asdict, astuple
from enum import Enum, IntEnum, Flag, auto
from typing import (
    TypeVar,
    Generic,
    Optional,
    Union,
    Literal,
    TypeAlias,
    TypeGuard,
    Any,
    ClassVar,
    Final,
    Callable,
    Iterator,
    Sequence,
    Mapping,
    get_type_hints,
    overload,
    NamedTuple,
)


# Dataclass
@dataclass(frozen=False, order=True, slots=True)
class Point3D:
    x: float
    y: float
    z: float = 0.0
    tags: list[str] = field(default_factory=list, compare=False)

    @property
    def magnitude(self) -> float:
        return (self.x**2 + self.y**2 + self.z**2) ** 0.5


p = Point3D(1, 2, 3, ["origin"])
print(f"  Dataclass: {p}, mag={p.magnitude:.2f}")
print(f"  asdict: {asdict(p)}")
print(f"  astuple: {astuple(p)}")


# Post-init
@dataclass
class Temperature:
    celsius: float
    fahrenheit: float = field(init=False)

    def __post_init__(self):
        self.fahrenheit = self.celsius * 9 / 5 + 32


t = Temperature(100)
print(f"  Post-init: {t.celsius}°C = {t.fahrenheit}°F")


# Enum
class Direction(Enum):
    NORTH = "N"
    SOUTH = "S"
    EAST = "E"
    WEST = "W"


class Permission(Flag):
    READ = auto()
    WRITE = auto()
    EXECUTE = auto()


class Priority(IntEnum):
    LOW = 1
    MEDIUM = 2
    HIGH = 3


print(f"  Enum: {Direction.NORTH}, value={Direction.NORTH.value}")
print(f"  Flag: {Permission.READ | Permission.WRITE}")
print(f"  IntEnum comparison: {Priority.HIGH > Priority.LOW}")


# NamedTuple (typed)
class Employee(NamedTuple):
    name: str
    department: str
    salary: float = 50_000


emp = Employee("Bob", "Engineering", 95_000)
print(f"  TypedNamedTuple: {emp.name}, ${emp.salary:,.0f}")

# Generics
T = TypeVar("T")
U = TypeVar("U")


class Stack(Generic[T]):
    def __init__(self) -> None:
        self._items: list[T] = []

    def push(self, item: T) -> None:
        self._items.append(item)

    def pop(self) -> T:
        return self._items.pop()

    def __repr__(self) -> str:
        return f"Stack({self._items})"


stack: Stack[int] = Stack()
stack.push(1)
stack.push(2)
stack.push(3)
print(f"  Generic Stack: {stack}, popped={stack.pop()}")

# Type aliases (3.10+)
Vector: TypeAlias = list[float]
Matrix: TypeAlias = list[Vector]


# Union types (3.10+ syntax)
def process(value: int | str | None) -> str:
    if value is None:
        return "nothing"
    return str(value)


# Literal types
def set_mode(mode: Literal["read", "write", "append"]) -> str:
    return f"Mode set to {mode}"


print(f"  Literal: {set_mode('read')}")


# TypeGuard
def is_str_list(val: list[Any]) -> TypeGuard[list[str]]:
    return all(isinstance(x, str) for x in val)


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 11: EXCEPTION HANDLING                                          ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§11 — EXCEPTION HANDLING")


# Full try/except/else/finally
def safe_divide(a: float, b: float) -> float | str:
    try:
        result = a / b
    except ZeroDivisionError as e:
        return f"Error: {e}"
    except TypeError:
        return "Error: wrong types"
    else:
        return result  # runs only if no exception
    finally:
        pass  # always runs (cleanup)


print(f"  10/3 = {safe_divide(10, 3):.4f}")
print(f"  10/0 = {safe_divide(10, 0)}")


# Custom exceptions with hierarchy
class AppError(Exception):
    """Base application error."""

    def __init__(self, message: str, code: int = 0):
        super().__init__(message)
        self.code = code


class ValidationError(AppError):
    pass


class NotFoundError(AppError):
    pass


try:
    raise ValidationError("Invalid email", code=422)
except AppError as e:
    print(f"  Custom exception: {e}, code={e.code}")

# Exception chaining
try:
    try:
        1 / 0
    except ZeroDivisionError as e:
        raise ValueError("Calculation failed") from e
except ValueError as e:
    print(f"  Chained: {e}, caused by {e.__cause__}")

# Exception groups (3.11+)
try:
    raise ExceptionGroup(
        "multiple errors",
        [
            ValueError("bad value"),
            TypeError("wrong type"),
            RuntimeError("runtime issue"),
        ],
    )
except* ValueError as eg:
    print(f"  ExceptionGroup caught ValueError: {eg.exceptions}")
except* TypeError as eg:
    print(f"  ExceptionGroup caught TypeError: {eg.exceptions}")
except* RuntimeError as eg:
    print(f"  ExceptionGroup caught RuntimeError: {eg.exceptions}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 12: CONTEXT MANAGERS                                            ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§12 — CONTEXT MANAGERS")


# Class-based context manager
class ManagedResource:
    def __init__(self, name: str):
        self.name = name

    def __enter__(self):
        print(f"  Acquiring {self.name}")
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        print(f"  Releasing {self.name}")
        return False  # don't suppress exceptions


with ManagedResource("database") as res:
    print(f"  Using {res.name}")

# Generator-based context manager
from contextlib import contextmanager


@contextmanager
def temp_directory(name: str):
    print(f"  Creating temp dir: {name}")
    try:
        yield name
    finally:
        print(f"  Cleaning up temp dir: {name}")


with temp_directory("/tmp/work") as d:
    print(f"  Working in {d}")

# Multiple context managers
from contextlib import ExitStack

with ExitStack() as stack:
    r1 = stack.enter_context(ManagedResource("lock_A"))
    r2 = stack.enter_context(ManagedResource("lock_B"))
    print(f"  Holding: {r1.name} and {r2.name}")

# Suppress exceptions
from contextlib import suppress

with suppress(FileNotFoundError):
    open("/nonexistent/file.txt")
    print("  This won't print")
print("  suppress: FileNotFoundError silently handled")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 13: ITERTOOLS, FUNCTOOLS & OPERATOR                            ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§13 — ITERTOOLS, FUNCTOOLS & OPERATOR")

import itertools
import operator

# itertools
print(f"  count:       {list(itertools.islice(itertools.count(10, 2), 5))}")
print(f"  cycle:       {list(itertools.islice(itertools.cycle('AB'), 6))}")
print(f"  repeat:      {list(itertools.repeat('x', 4))}")
print(f"  chain:       {list(itertools.chain([1, 2], [3, 4], [5]))}")
print(f"  product:     {list(itertools.product('AB', '12'))}")
print(f"  permutations:{list(itertools.permutations('ABC', 2))[:4]}...")
print(f"  combinations:{list(itertools.combinations('ABCD', 2))}")
print(f"  accumulate:  {list(itertools.accumulate([1, 2, 3, 4, 5]))}")
print(f"  starmap:     {list(itertools.starmap(pow, [(2, 3), (3, 2), (10, 3)]))}")

# groupby
data = [("fruit", "apple"), ("fruit", "banana"), ("veg", "carrot"), ("veg", "daikon")]
for key, group in itertools.groupby(data, key=lambda x: x[0]):
    print(f"  groupby {key}: {[item[1] for item in group]}")

# functools
from functools import partial, singledispatch, total_ordering

add_ten = partial(operator.add, 10)
print(f"  partial: add_ten(5) = {add_ten(5)}")


# singledispatch (function overloading by type)
@singledispatch
def process_data(data):
    return f"Unknown type: {type(data).__name__}"


@process_data.register(int)
def _(data):
    return f"Integer: {data * 2}"


@process_data.register(str)
def _(data):
    return f"String: {data.upper()}"


@process_data.register(list)
def _(data):
    return f"List of {len(data)} items"


print(f"  singledispatch(42):     {process_data(42)}")
print(f"  singledispatch('hi'):   {process_data('hi')}")
print(f"  singledispatch([1,2]):  {process_data([1, 2])}")


# total_ordering
@total_ordering
class Student:
    def __init__(self, name: str, grade: float):
        self.name, self.grade = name, grade

    def __eq__(self, other):
        return self.grade == other.grade

    def __lt__(self, other):
        return self.grade < other.grade


s1, s2 = Student("Alice", 90), Student("Bob", 85)
print(f"  total_ordering: {s1.name} > {s2.name}: {s1 > s2}")

# operator module
print(f"  operator.mul: {operator.mul(6, 7)}")
print(f"  itemgetter: {operator.itemgetter(1, 3)([10, 20, 30, 40])}")
print(f"  attrgetter: {operator.attrgetter('name')(s1)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 14: STRING FEATURES                                             ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§14 — STRING FEATURES")

s = "  Hello, Python World!  "
print(f"  strip:      '{s.strip()}'")
print(f"  split:      {s.split()}")
print(f"  join:       {'-'.join(['a', 'b', 'c'])}")
print(f"  replace:    {s.strip().replace('World', 'Universe')}")
print(f"  startswith: {s.strip().startswith('Hello')}")
print(f"  find:       {s.find('Python')}")
print(f"  count:      {'banana'.count('a')}")
print(f"  isdigit:    {'123'.isdigit()}, isalpha: {'abc'.isalpha()}")
print(f"  center:     '{'hi':^20}'")
print(f"  zfill:      {'42'.zfill(8)}")
print(f"  title:      {'hello world'.title()}")
print(f"  partition:  {'key=value'.partition('=')}")
print(f"  encode:     {'café'.encode('utf-8')}")

# String formatting
print(f"  %:          {'%s has %d items' % ('list', 5)}")
print(f"  format:     {'{name} is {age}'.format(name='Alice', age=30)}")
print(f"  f-string:   {42:08b} (binary), {255:#06x} (hex)")
print(f"  !r !s !a:   {'café'!r}, {'café'!s}, {'café'!a}")

# Regular expressions
import re

text = "Call 555-1234 or 555-5678. Email: user@example.com"
phones = re.findall(r"\d{3}-\d{4}", text)
email = re.search(r"[\w.]+@[\w.]+", text)
replaced = re.sub(r"\d{3}-\d{4}", "XXX-XXXX", text)
print(f"  regex findall: {phones}")
print(f"  regex search:  {email.group() if email else None}")
print(f"  regex sub:     {replaced}")

# Compiled regex with named groups
pattern = re.compile(r"(?P<area>\d{3})-(?P<number>\d{4})")
m = pattern.search("555-1234")
print(f"  named groups:  area={m.group('area')}, number={m.group('number')}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 15: FILE I/O & PATHLIB                                          ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§15 — FILE I/O & PATHLIB")

from pathlib import Path
import tempfile, json, csv, pickle, io

# Pathlib
p = Path("/tmp/demo")
p.mkdir(exist_ok=True)
print(f"  Path: {p}, exists={p.exists()}, is_dir={p.is_dir()}")
print(f"  Parts: {Path('/usr/local/bin/python').parts}")
print(f"  Stem/suffix: {Path('data.tar.gz').stem}, {Path('data.tar.gz').suffixes}")
print(f"  Home: {Path.home()}")

# Text file I/O
test_file = p / "test.txt"
test_file.write_text("Hello\nWorld\nPython", encoding="utf-8")
content = test_file.read_text(encoding="utf-8")
print(f"  File content: {content.splitlines()}")

# Line-by-line reading
with open(test_file) as f:
    lines = [line.strip() for line in f]
print(f"  Lines: {lines}")

# JSON
data = {"name": "Alice", "scores": [95, 87, 92], "active": True}
json_file = p / "data.json"
json_file.write_text(json.dumps(data, indent=2))
loaded = json.loads(json_file.read_text())
print(f"  JSON roundtrip: {loaded}")

# CSV
csv_file = p / "data.csv"
with open(csv_file, "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=["name", "age"])
    writer.writeheader()
    writer.writerows([{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}])

with open(csv_file) as f:
    reader = csv.DictReader(f)
    rows = list(reader)
print(f"  CSV: {rows}")

# Pickle
pickle_file = p / "data.pkl"
with open(pickle_file, "wb") as f:
    pickle.dump({"key": [1, 2, 3]}, f)
with open(pickle_file, "rb") as f:
    unpickled = pickle.load(f)
print(f"  Pickle: {unpickled}")

# StringIO / BytesIO
sio = io.StringIO()
sio.write("in-memory text")
print(f"  StringIO: {sio.getvalue()}")

# Glob
txt_files = list(p.glob("*.txt"))
print(f"  Glob *.txt: {[f.name for f in txt_files]}")

# Cleanup
import shutil

shutil.rmtree(p)
print(f"  Cleaned up {p}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 16: CONCURRENCY & PARALLELISM                                  ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§16 — CONCURRENCY & PARALLELISM")

import threading
import multiprocessing
import concurrent.futures
import asyncio
import queue

# Threading
results = []
lock = threading.Lock()


def worker(n: int):
    with lock:
        results.append(n * n)


threads = [threading.Thread(target=worker, args=(i,)) for i in range(5)]
for t in threads:
    t.start()
for t in threads:
    t.join()
print(f"  Threading results: {sorted(results)}")

# Thread-safe queue
q = queue.Queue()
for i in range(5):
    q.put(i)
items = [q.get() for _ in range(5)]
print(f"  Queue: {items}")

# ThreadPoolExecutor
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:
    futures = {executor.submit(lambda x: x**2, i): i for i in range(5)}
    pool_results = []
    for future in concurrent.futures.as_completed(futures):
        pool_results.append(future.result())
print(f"  ThreadPool: {sorted(pool_results)}")


# ProcessPoolExecutor (picklable function needed)
def square_func(x):
    return x**2


with concurrent.futures.ProcessPoolExecutor(max_workers=2) as executor:
    proc_results = list(executor.map(square_func, range(5)))
print(f"  ProcessPool: {proc_results}")


# Asyncio
async def async_task(name: str, delay: float) -> str:
    await asyncio.sleep(delay)
    return f"{name} done"


async def async_main():
    # Gather multiple coroutines
    results = await asyncio.gather(
        async_task("A", 0.01),
        async_task("B", 0.02),
        async_task("C", 0.01),
    )
    return results


# async for / async with demonstration
class AsyncCounter:
    def __init__(self, stop: int):
        self.stop = stop

    def __aiter__(self):
        self.current = 0
        return self

    async def __anext__(self):
        if self.current >= self.stop:
            raise StopAsyncIteration
        self.current += 1
        await asyncio.sleep(0)
        return self.current


class AsyncResource:
    async def __aenter__(self):
        return self

    async def __aexit__(self, *args):
        pass

    async def data(self):
        return "async resource data"


async def async_features():
    # async for
    items = []
    async for i in AsyncCounter(5):
        items.append(i)

    # async with
    async with AsyncResource() as res:
        d = await res.data()

    # async comprehension
    values = [i async for i in AsyncCounter(3)]

    return items, d, values


loop_results = asyncio.run(async_main())
print(f"  asyncio.gather: {loop_results}")

async_feat = asyncio.run(async_features())
print(f"  async for: {async_feat[0]}")
print(f"  async with: {async_feat[1]}")
print(f"  async comp: {async_feat[2]}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 17: DESCRIPTORS & METAPROGRAMMING                              ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§17 — DESCRIPTORS & METAPROGRAMMING")


# Descriptor protocol
class Validated:
    """Data descriptor that validates values."""

    def __init__(self, min_val: float, max_val: float):
        self.min_val = min_val
        self.max_val = max_val
        self.name = None

    def __set_name__(self, owner, name):
        self.name = name

    def __get__(self, obj, objtype=None):
        if obj is None:
            return self
        return obj.__dict__.get(self.name, 0)

    def __set__(self, obj, value):
        if not self.min_val <= value <= self.max_val:
            raise ValueError(
                f"{self.name} must be between {self.min_val} and {self.max_val}"
            )
        obj.__dict__[self.name] = value


class Config:
    temperature = Validated(0, 100)
    humidity = Validated(0, 100)


cfg = Config()
cfg.temperature = 72
cfg.humidity = 45
print(f"  Descriptor: temp={cfg.temperature}, humid={cfg.humidity}")

try:
    cfg.temperature = 150
except ValueError as e:
    print(f"  Descriptor validation: {e}")


# Metaclass
class SingletonMeta(type):
    _instances = {}

    def __call__(cls, *args, **kwargs):
        if cls not in cls._instances:
            cls._instances[cls] = super().__call__(*args, **kwargs)
        return cls._instances[cls]


class Database(metaclass=SingletonMeta):
    def __init__(self):
        self.connected = True


db1 = Database()
db2 = Database()
print(f"  Metaclass singleton: db1 is db2 = {db1 is db2}")


# __init_subclass__
class Plugin:
    _registry = {}

    def __init_subclass__(cls, plugin_name: str = None, **kwargs):
        super().__init_subclass__(**kwargs)
        name = plugin_name or cls.__name__
        Plugin._registry[name] = cls


class JsonPlugin(Plugin, plugin_name="json"):
    pass


class XmlPlugin(Plugin, plugin_name="xml"):
    pass


print(f"  __init_subclass__ registry: {list(Plugin._registry.keys())}")


# __class_getitem__ (generic-like syntax)
class TypedBox:
    def __class_getitem__(cls, item):
        return f"TypedBox[{item.__name__}]"


print(f"  __class_getitem__: {TypedBox[int]}")


# Dynamic attribute access
class DynamicAttrs:
    def __getattr__(self, name):
        return f"<dynamic:{name}>"

    def __setattr__(self, name, value):
        super().__setattr__(name, value)


dyn = DynamicAttrs()
print(f"  __getattr__: {dyn.anything} {dyn.whatever}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 18: MAGIC / DUNDER METHODS DEEP DIVE                           ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§18 — MAGIC / DUNDER METHODS DEEP DIVE")


class Vector2D:
    """Demonstrates numeric and container dunders."""

    def __init__(self, x: float, y: float):
        self.x, self.y = x, y

    # String representations
    def __repr__(self):
        return f"Vector2D({self.x}, {self.y})"

    def __str__(self):
        return f"({self.x}, {self.y})"

    def __format__(self, spec):
        return f"({self.x:{spec}}, {self.y:{spec}})"

    # Arithmetic
    def __add__(self, other):
        return Vector2D(self.x + other.x, self.y + other.y)

    def __sub__(self, other):
        return Vector2D(self.x - other.x, self.y - other.y)

    def __mul__(self, scalar):
        return Vector2D(self.x * scalar, self.y * scalar)

    def __rmul__(self, scalar):
        return self.__mul__(scalar)

    def __truediv__(self, scalar):
        return Vector2D(self.x / scalar, self.y / scalar)

    def __neg__(self):
        return Vector2D(-self.x, -self.y)

    def __abs__(self):
        return (self.x**2 + self.y**2) ** 0.5

    def __matmul__(self, other):
        return self.x * other.x + self.y * other.y  # dot product

    # Comparison
    def __eq__(self, other):
        return self.x == other.x and self.y == other.y

    def __lt__(self, other):
        return abs(self) < abs(other)

    def __bool__(self):
        return self.x != 0 or self.y != 0

    # Container-like
    def __getitem__(self, index):
        return (self.x, self.y)[index]

    def __iter__(self):
        yield self.x
        yield self.y

    def __len__(self):
        return 2

    def __reversed__(self):
        return iter((self.y, self.x))

    # Hashing
    def __hash__(self):
        return hash((self.x, self.y))


v1 = Vector2D(3, 4)
v2 = Vector2D(1, 2)
print(f"  v1 + v2 = {v1 + v2}")
print(f"  v1 * 3 = {v1 * 3}")
print(f"  3 * v1 = {3 * v1}")
print(f"  |v1| = {abs(v1)}")
print(f"  v1 @ v2 (dot) = {v1 @ v2}")
print(f"  format: {v1:.2f}")
print(f"  unpack: x={v1[0]}, y={v1[1]}")
print(f"  iter: {list(v1)}")
print(f"  reversed: {list(reversed(v1))}")
print(f"  bool(Vector2D(0,0)): {bool(Vector2D(0, 0))}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 19: INTROSPECTION & REFLECTION                                 ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§19 — INTROSPECTION & REFLECTION")

import inspect


class Sample:
    """A sample class."""

    class_var = 42

    def method(self, x: int) -> str:
        """A method."""
        return str(x)


# type() and isinstance()
print(f"  type(42): {type(42)}")
print(f"  isinstance check: {isinstance(42, (int, float))}")
print(f"  issubclass: {issubclass(bool, int)}")

# dir() and vars()
print(f"  dir (filtered): {[a for a in dir(Sample) if not a.startswith('_')]}")
print(f"  vars: {vars(Sample)['class_var']}")

# getattr, setattr, hasattr, delattr
s = Sample()
setattr(s, "dynamic", 99)
print(f"  getattr: {getattr(s, 'dynamic')}")
print(f"  hasattr: {hasattr(s, 'dynamic')}")

# inspect module
print(f"  Source file: {inspect.getfile(Sample)[:40]}...")
sig = inspect.signature(Sample.method)
print(f"  Signature: {sig}")
print(f"  Parameters: {list(sig.parameters.keys())}")
print(f"  Is class: {inspect.isclass(Sample)}")
print(f"  Is function: {inspect.isfunction(Sample.method)}")

# Type hints at runtime
hints = get_type_hints(Sample.method)
print(f"  Type hints: {hints}")

# __dict__ vs __slots__
print(f"  Instance __dict__: {s.__dict__}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 20: CLOSURES, SCOPING & ADVANCED FUNCTIONS                     ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§20 — CLOSURES, SCOPING & ADVANCED FUNCTIONS")

# LEGB rule demonstration
global_var = "global"


def outer():
    enclosing_var = "enclosing"

    def inner():
        local_var = "local"
        print(f"  LEGB: {local_var}, {enclosing_var}, {global_var}")

    inner()


outer()


# nonlocal keyword
def counter():
    count = 0

    def increment():
        nonlocal count
        count += 1
        return count

    return increment


c = counter()
print(f"  nonlocal: {c()}, {c()}, {c()}")

# global keyword
_global_demo = 0


def modify_global():
    global _global_demo
    _global_demo = 42


modify_global()
print(f"  global: {_global_demo}")


# Callable objects and __call__
class Adder:
    def __init__(self, n):
        self.n = n

    def __call__(self, x):
        return self.n + x


add5 = Adder(5)
print(f"  __call__: {add5(10)}")


# Function as first-class object
def apply(func, value):
    return func(value)


print(f"  First-class: {apply(len, 'hello')}")
print(f"  First-class: {apply(sorted, [3, 1, 2])}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 21: UNPACKING & ASSIGNMENT FEATURES                            ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§21 — UNPACKING & ASSIGNMENT FEATURES")

# Basic unpacking
a, b, c = [1, 2, 3]
print(f"  Basic: a={a}, b={b}, c={c}")

# Star unpacking
first, *middle, last = [1, 2, 3, 4, 5]
print(f"  Star: first={first}, middle={middle}, last={last}")

# Nested unpacking
(a, b), (c, d) = (1, 2), (3, 4)
print(f"  Nested: {a}, {b}, {c}, {d}")

# Swap
x, y = 10, 20
x, y = y, x
print(f"  Swap: x={x}, y={y}")

# Dict unpacking
defaults = {"color": "red", "size": 10}
overrides = {"size": 20, "shape": "circle"}
merged = {**defaults, **overrides}
print(f"  Dict merge: {merged}")


# Function arg unpacking
def show(a, b, c):
    return f"{a}-{b}-{c}"


args = [1, 2, 3]
kwargs = {"a": "x", "b": "y", "c": "z"}
print(f"  *args unpack: {show(*args)}")
print(f"  **kwargs unpack: {show(**kwargs)}")

# Augmented assignment
x = 10
x += 5
x -= 2
x *= 3
x //= 4
print(f"  Augmented: {x}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 22: ITERATORS & ITERATION PROTOCOL                             ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§22 — ITERATORS & ITERATION PROTOCOL")


# Custom iterator
class Countdown:
    def __init__(self, start: int):
        self.current = start

    def __iter__(self):
        return self

    def __next__(self):
        if self.current <= 0:
            raise StopIteration
        self.current -= 1
        return self.current + 1


print(f"  Custom iterator: {list(Countdown(5))}")


# Infinite iterator with islice
class Naturals:
    def __init__(self):
        self.n = 0

    def __iter__(self):
        return self

    def __next__(self):
        self.n += 1
        return self.n


print(f"  Infinite: {list(itertools.islice(Naturals(), 7))}")

# iter() with sentinel
import io

stream = io.StringIO("line1\nline2\nline3\nSTOP\nline4")
lines = list(iter(stream.readline, "STOP\n"))
print(f"  Sentinel iter: {[l.strip() for l in lines]}")

# zip, enumerate, reversed
names = ["Alice", "Bob", "Charlie"]
scores = [95, 87, 92]
print(f"  zip: {list(zip(names, scores))}")
print(f"  zip strict: ", end="")
try:
    list(zip([1, 2], [3, 4, 5], strict=True))
except ValueError as e:
    print(f"{e}")
print(f"  enumerate: {list(enumerate(names, start=1))}")
print(f"  reversed: {list(reversed(names))}")

# any() and all()
print(f"  any([0, '', None, 42]): {any([0, '', None, 42])}")
print(f"  all([1, True, 'yes']): {all([1, True, 'yes'])}")

# sorted with key
words = ["banana", "apple", "Cherry", "date"]
print(f"  sorted: {sorted(words, key=str.lower)}")
print(f"  min/max: {min(words, key=len)}, {max(words, key=len)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 23: MEMORY & PERFORMANCE                                        ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§23 — MEMORY & PERFORMANCE")

import sys
import weakref
from timeit import timeit

# sys.getsizeof
print(f"  sizeof int:   {sys.getsizeof(0)} bytes")
print(f"  sizeof str:   {sys.getsizeof('hello')} bytes")
print(f"  sizeof list:  {sys.getsizeof([1, 2, 3])} bytes")
print(f"  sizeof dict:  {sys.getsizeof({1: 2, 3: 4})} bytes")
print(f"  sizeof tuple: {sys.getsizeof((1, 2, 3))} bytes")


# __slots__ vs __dict__ memory
class WithDict:
    def __init__(self):
        self.x = 1
        self.y = 2


class WithSlots:
    __slots__ = ("x", "y")

    def __init__(self):
        self.x = 1
        self.y = 2


print(f"  __dict__ size: {sys.getsizeof(WithDict().__dict__)} bytes")
print(f"  __slots__ (no __dict__): slots object is more compact")


# Weak references
class Cacheable:
    def __init__(self, val):
        self.val = val


obj = Cacheable(42)
ref = weakref.ref(obj)
print(f"  Weakref alive: {ref() is not None}, val={ref().val}")
del obj
print(f"  Weakref after del: {ref()}")

# timeit
t = timeit("sum(range(1000))", number=10000)
print(f"  timeit sum(range(1000)): {t:.4f}s for 10k runs")

# Memory view
data = bytearray(b"Hello World")
mv = memoryview(data)
mv[6:11] = b"Earth"
print(f"  memoryview: {data}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 24: STANDARD LIBRARY HIGHLIGHTS                                 ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§24 — STANDARD LIBRARY HIGHLIGHTS")

# datetime
from datetime import datetime, timedelta, date, timezone

now = datetime.now()
utc_now = datetime.now(timezone.utc)
print(f"  datetime: {now.strftime('%Y-%m-%d %H:%M:%S')}")
print(f"  timedelta: {now + timedelta(days=30):%Y-%m-%d}")

# math
import math

print(
    f"  math: ceil={math.ceil(3.2)}, floor={math.floor(3.8)}, "
    f"gcd={math.gcd(48, 18)}, factorial={math.factorial(6)}"
)
print(
    f"  math: sqrt={math.sqrt(2):.4f}, log={math.log(math.e):.4f}, "
    f"sin={math.sin(math.pi / 2):.4f}"
)

# statistics
import statistics

data = [2, 5, 1, 8, 3, 7, 4, 6]
print(
    f"  stats: mean={statistics.mean(data):.1f}, median={statistics.median(data)}, "
    f"stdev={statistics.stdev(data):.2f}"
)

# random
import random

random.seed(42)
print(
    f"  random: int={random.randint(1, 100)}, choice={random.choice(names)}, "
    f"shuffle={random.sample(range(10), 5)}"
)

# hashlib
import hashlib

h = hashlib.sha256(b"hello world").hexdigest()
print(f"  sha256: {h[:32]}...")

# uuid
import uuid

print(f"  uuid4: {uuid.uuid4()}")

# decimal & fractions
from decimal import Decimal, getcontext
from fractions import Fraction

getcontext().prec = 50
print(f"  Decimal: {Decimal('1') / Decimal('3')}")
print(
    f"  Fraction: {Fraction(1, 3) + Fraction(1, 6)} = {float(Fraction(1, 3) + Fraction(1, 6)):.4f}"
)

# copy
import copy

original = [[1, 2], [3, 4]]
shallow = copy.copy(original)
deep = copy.deepcopy(original)
original[0].append(999)
print(f"  shallow copy affected: {shallow[0]}")
print(f"  deep copy unaffected:  {deep[0]}")

# textwrap
import textwrap

long_text = "This is a very long line of text that should be wrapped to fit within a reasonable column width for display."
print(
    f"  textwrap:\n{textwrap.fill(long_text, width=50, initial_indent='    ', subsequent_indent='    ')}"
)

# pprint
from pprint import pformat

complex_data = {
    "users": [
        {"name": "Alice", "roles": ["admin", "user"]},
        {"name": "Bob", "roles": ["user"]},
    ]
}
print(f"  pformat: {pformat(complex_data, width=60)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 25: STRUCTURAL PATTERN MATCHING (ADVANCED)                      ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§25 — STRUCTURAL PATTERN MATCHING (ADVANCED)")


# Class patterns
@dataclass
class Command:
    action: str
    target: str
    args: list = field(default_factory=list)


def execute(cmd: Command) -> str:
    match cmd:
        case Command(action="move", target=t, args=[x, y]):
            return f"Moving {t} to ({x}, {y})"
        case Command(action="delete", target=t):
            return f"Deleting {t}"
        case Command(action="copy", target=t, args=[dest]):
            return f"Copying {t} to {dest}"
        case Command(action=a):
            return f"Unknown action: {a}"


commands = [
    Command("move", "file.txt", [10, 20]),
    Command("delete", "temp.log"),
    Command("copy", "data.csv", ["/backup"]),
    Command("archive", "project"),
]
for cmd in commands:
    print(f"  {execute(cmd)}")


# OR patterns and AS patterns
def categorize(item):
    match item:
        case "red" | "blue" | "green" as color:
            return f"color: {color}"
        case int() | float() as num if num > 0:
            return f"positive number: {num}"
        case [*items] if len(items) > 3:
            return f"long sequence: {len(items)} items"
        case {"type": "error", "message": msg}:
            return f"error: {msg}"
        case _:
            return "uncategorized"


for item in ["red", 42, -5, [1, 2, 3, 4, 5], {"type": "error", "message": "oops"}]:
    print(f"  categorize({item!r:>35}) → {categorize(item)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 26: TYPE SYSTEM ADVANCED FEATURES                              ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§26 — TYPE SYSTEM ADVANCED")

from typing import (
    ParamSpec,
    Concatenate,
    TypeVarTuple,
    Unpack,
    Self,
    Never,
    assert_type,
    reveal_type,
    TYPE_CHECKING,
)

# ParamSpec (preserving callable signatures)
P = ParamSpec("P")
R = TypeVar("R")


def logged(func: Callable[P, R]) -> Callable[P, R]:
    @functools.wraps(func)
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
        print(f"  [logged] Calling {func.__name__}")
        return func(*args, **kwargs)

    return wrapper


@logged
def add_nums(a: int, b: int) -> int:
    return a + b


print(f"  ParamSpec result: {add_nums(3, 4)}")


# Self type (3.11+)
class Builder:
    def __init__(self):
        self.parts: list[str] = []

    def add(self, part: str) -> Self:
        self.parts.append(part)
        return self

    def build(self) -> str:
        return " + ".join(self.parts)


result = Builder().add("engine").add("wheels").add("body").build()
print(f"  Self type (builder): {result}")

# TypeVar with bounds and constraints
from typing import TypeVar

Numeric = TypeVar("Numeric", int, float)


def double(x: Numeric) -> Numeric:
    return x * 2


print(f"  Constrained TypeVar: {double(5)}, {double(3.14)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 27: TESTING PATTERNS                                            ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§27 — TESTING PATTERNS")


# Doctest
def add_positive(a: int, b: int) -> int:
    """
    Add two positive integers.

    >>> add_positive(2, 3)
    5
    >>> add_positive(-1, 2)
    Traceback (most recent call last):
        ...
    ValueError: Both numbers must be positive
    """
    if a < 0 or b < 0:
        raise ValueError("Both numbers must be positive")
    return a + b


# Assert statements
assert add_positive(2, 3) == 5
assert isinstance(add_positive(1, 1), int)
print("  Assertions passed!")


# Simple test framework pattern
class TestResults:
    passed = 0
    failed = 0

    @classmethod
    def check(cls, name: str, condition: bool):
        if condition:
            cls.passed += 1
        else:
            cls.failed += 1
            print(f"  FAILED: {name}")


TestResults.check("addition", 2 + 2 == 4)
TestResults.check("string", "hello".upper() == "HELLO")
TestResults.check("list", len([1, 2, 3]) == 3)
TestResults.check("dict", {"a": 1}.get("b", 0) == 0)
print(f"  Tests: {TestResults.passed} passed, {TestResults.failed} failed")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 28: ADVANCED PATTERNS & IDIOMS                                 ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§28 — ADVANCED PATTERNS & IDIOMS")


# Mixin pattern
class JsonMixin:
    def to_json(self) -> str:
        return json.dumps(self.__dict__)


class LogMixin:
    def log(self, msg: str):
        return f"[{self.__class__.__name__}] {msg}"


class User(JsonMixin, LogMixin):
    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age


u = User("Alice", 30)
print(f"  Mixin json: {u.to_json()}")
print(f"  Mixin log: {u.log('created')}")

# Strategy pattern
from typing import Callable


def sort_strategy(data: list, strategy: Callable[[list], list]) -> list:
    return strategy(data)


print(f"  Strategy (sorted): {sort_strategy([3, 1, 2], sorted)}")
print(
    f"  Strategy (reverse): {sort_strategy([3, 1, 2], lambda x: sorted(x, reverse=True))}"
)


# Observer pattern
class EventEmitter:
    def __init__(self):
        self._listeners: dict[str, list[Callable]] = defaultdict(list)

    def on(self, event: str, callback: Callable):
        self._listeners[event].append(callback)

    def emit(self, event: str, *args):
        for cb in self._listeners[event]:
            cb(*args)


emitter = EventEmitter()
log = []
emitter.on("data", lambda x: log.append(f"received: {x}"))
emitter.on("data", lambda x: log.append(f"processed: {x.upper()}"))
emitter.emit("data", "hello")
print(f"  Observer: {log}")


# Flyweight with __new__
class Flyweight:
    _cache: dict = {}

    def __new__(cls, value: str):
        if value not in cls._cache:
            instance = super().__new__(cls)
            instance.value = value
            cls._cache[value] = instance
        return cls._cache[value]


f1 = Flyweight("shared")
f2 = Flyweight("shared")
print(f"  Flyweight: f1 is f2 = {f1 is f2}")


# Method chaining (fluent interface)
class Query:
    def __init__(self):
        self._parts = []

    def select(self, *fields) -> Query:
        self._parts.append(f"SELECT {', '.join(fields)}")
        return self

    def where(self, condition: str) -> Query:
        self._parts.append(f"WHERE {condition}")
        return self

    def build(self) -> str:
        return " ".join(self._parts)


sql = Query().select("name", "age").where("age > 18").build()
print(f"  Fluent: {sql}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 29: DUNDERS GRAB BAG                                            ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§29 — DUNDERS GRAB BAG")


# __missing__ for dicts
class DefaultMap(dict):
    def __missing__(self, key):
        self[key] = f"default_{key}"
        return self[key]


dm = DefaultMap({"a": 1})
print(f"  __missing__: {dm['b']}, {dict(dm)}")


# __enter__/__exit__ with returns
class Timer:
    def __enter__(self):
        self.start = time.perf_counter()
        return self

    def __exit__(self, *args):
        self.elapsed = time.perf_counter() - self.start


with Timer() as t:
    sum(range(100_000))
print(f"  Timer context: {t.elapsed:.6f}s")


# __del__ (destructor)
class Destructor:
    def __del__(self):
        pass  # cleanup here


# __sizeof__
class Compact:
    __slots__ = ("data",)

    def __init__(self):
        self.data = 42

    def __sizeof__(self):
        return super().__sizeof__()


print(f"  __sizeof__: {sys.getsizeof(Compact())} bytes")

# __subclasshook__ (ABC)
from abc import ABCMeta


class MyInterface(metaclass=ABCMeta):
    @classmethod
    def __subclasshook__(cls, C):
        if any("do_work" in B.__dict__ for B in C.__mro__):
            return True
        return NotImplemented


class Worker:
    def do_work(self):
        pass


print(f"  __subclasshook__: {isinstance(Worker(), MyInterface)}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 30: WALRUS, ASSIGNMENT EXPRESSIONS & MISC                      ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§30 — MISC FEATURES")

# Walrus in while loops
import io

stream = io.StringIO("line1\nline2\nline3\n")
lines = []
while line := stream.readline():
    lines.append(line.strip())
print(f"  Walrus while: {lines}")

# Walrus in list comprehensions
results = [y for x in range(10) if (y := x**2) > 20]
print(f"  Walrus comp: {results}")


# Ellipsis
def todo(): ...  # same as pass, used as placeholder


print(f"  Ellipsis: {type(...).__name__}, {... is Ellipsis}")

# Underscores
_ = "throwaway"
for _ in range(3):  # unused loop variable
    pass

# Multiple assignment targets
a = b = c = 0
print(f"  Multiple assign: a={a}, b={b}, c={c}")

# Assert with message
try:
    assert 1 == 2, "One does not equal two"
except AssertionError as e:
    print(f"  Assert msg: {e}")

# exec and eval
eval_result = eval("2 + 3 * 4")
print(f"  eval: {eval_result}")

namespace = {}
exec("def greet(name): return f'Hello {name}'", namespace)
print(f"  exec: {namespace['greet']('World')}")

# compile
code = compile("x = 42; print(f'  compiled: x={x}')", "<string>", "exec")
exec(code)

# __all__ (module export control)
__all__ = ["Animal", "Dog", "Vector2D"]  # convention for module exports

# sys features
print(f"  sys.platform: {sys.platform}")
print(f"  sys.maxsize: {sys.maxsize}")
print(f"  sys.recursionlimit: {sys.getrecursionlimit()}")
print(f"  sys.float_info.max: {sys.float_info.max}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 31: BISECT, HEAPQ & SPECIALIZED DATA STRUCTURES                ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§31 — BISECT, HEAPQ & DATA STRUCTURES")

import bisect
import heapq
from array import array
from collections import UserDict, UserList

# bisect (binary search on sorted lists)
sorted_list = [1, 3, 5, 7, 9, 11]
pos = bisect.bisect_left(sorted_list, 6)
bisect.insort(sorted_list, 6)
print(f"  bisect: insert 6 at pos {pos} → {sorted_list}")

# heapq (min-heap)
heap = [5, 3, 8, 1, 9, 2]
heapq.heapify(heap)
print(f"  heapq: smallest={heapq.heappop(heap)}, nlargest={heapq.nlargest(3, heap)}")

# Priority queue via heapq
pq = []
heapq.heappush(pq, (2, "medium"))
heapq.heappush(pq, (1, "high"))
heapq.heappush(pq, (3, "low"))
print(f"  PriorityQ: {heapq.heappop(pq)}")

# array (typed, compact)
int_array = array("i", [1, 2, 3, 4, 5])
print(f"  array: {int_array}, typecode={int_array.typecode}")


# UserDict / UserList (for subclassing)
class CaseInsensitiveDict(UserDict):
    def __setitem__(self, key, value):
        super().__setitem__(key.lower(), value)

    def __getitem__(self, key):
        return super().__getitem__(key.lower())


cid = CaseInsensitiveDict()
cid["Name"] = "Alice"
print(f"  UserDict: {cid['name']} (case-insensitive)")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 32: OS, SUBPROCESS & SYSTEM INTERACTION                        ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§32 — OS & SYSTEM INTERACTION")

import os
import subprocess
import platform

print(f"  Platform: {platform.system()} {platform.release()}")
print(f"  Python: {platform.python_version()}")
print(f"  CWD: {os.getcwd()}")
print(f"  PID: {os.getpid()}")
print(f"  CPU count: {os.cpu_count()}")
print(f"  Env HOME: {os.environ.get('HOME', 'N/A')}")

# subprocess
result = subprocess.run(
    ["echo", "Hello from subprocess"], capture_output=True, text=True
)
print(f"  subprocess: {result.stdout.strip()}")

# os.path vs pathlib
print(f"  os.path.join: {os.path.join('/usr', 'local', 'bin')}")
print(f"  pathlib:      {Path('/usr') / 'local' / 'bin'}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 33: LOGGING & WARNINGS                                          ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§33 — LOGGING & WARNINGS")

import logging
import warnings

# Logging setup
logger = logging.getLogger("showcase")
handler = logging.StreamHandler()
handler.setFormatter(logging.Formatter("  [%(levelname)s] %(message)s"))
logger.addHandler(handler)
logger.setLevel(logging.DEBUG)

logger.debug("Debug message")
logger.info("Info message")
logger.warning("Warning message")

# Warnings
with warnings.catch_warnings(record=True) as w:
    warnings.simplefilter("always")
    warnings.warn("This is deprecated", DeprecationWarning)
    print(f"  Warning caught: {w[0].category.__name__}: {w[0].message}")

# Remove handler to prevent duplicate output
logger.removeHandler(handler)


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 34: STRUCT, BYTES & BINARY DATA                                ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§34 — STRUCT, BYTES & BINARY DATA")

import struct
import base64
import binascii

# struct (C-compatible binary packing)
packed = struct.pack(">ihf", 42, 1000, 3.14)
unpacked = struct.unpack(">ihf", packed)
print(f"  struct pack/unpack: {unpacked}")

# bytes operations
data = bytes([72, 101, 108, 108, 111])
print(f"  bytes: {data}, hex={data.hex()}")
print(f"  from hex: {bytes.fromhex('48656c6c6f')}")

# base64
encoded = base64.b64encode(b"Hello World")
decoded = base64.b64decode(encoded)
print(f"  base64: {encoded} → {decoded}")

# bytearray (mutable bytes)
ba = bytearray(b"Hello")
ba[0] = 74  # 'J'
print(f"  bytearray: {ba}")

# int to/from bytes
n = 1024
as_bytes = n.to_bytes(4, byteorder="big")
back = int.from_bytes(as_bytes, byteorder="big")
print(f"  int bytes: {as_bytes.hex()} → {back}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 35: ABSTRACT SYNTAX TREE (AST) & CODE OBJECTS                  ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§35 — AST & CODE OBJECTS")

import ast
import dis

# Parse and analyze code
code_str = "x = 1 + 2 * 3"
tree = ast.parse(code_str)
print(f"  AST: {ast.dump(tree, indent=2)[:80]}...")


# Simple AST visitor
class NameCollector(ast.NodeVisitor):
    def __init__(self):
        self.names = []

    def visit_Name(self, node):
        self.names.append(node.id)
        self.generic_visit(node)


collector = NameCollector()
collector.visit(ast.parse("result = x + y * z"))
print(f"  AST names: {collector.names}")


# Disassemble bytecode
def simple_func(a, b):
    return a + b


print("  Bytecode for 'a + b':")
dis.dis(simple_func)

# Code object inspection
code_obj = simple_func.__code__
print(f"  Code: args={code_obj.co_argcount}, vars={code_obj.co_varnames}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 36: __future__ & VERSION-SPECIFIC FEATURES                     ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§36 — VERSION-SPECIFIC FEATURES")

# from __future__ import annotations (PEP 563) - at top of file
# Enables postponed evaluation of annotations

# Python 3.8+: Walrus operator (:=) — shown in §3
# Python 3.9+: dict merge (|=) — shown in §2
# Python 3.10+: match/case — shown in §4, §25
# Python 3.10+: Union type syntax (X | Y) — shown in §10
# Python 3.11+: ExceptionGroup — shown in §11
# Python 3.11+: Self type — shown in §26
# Python 3.12+: type statement
# type Point = tuple[float, float]  # 3.12+ only

print("  Version-specific features demonstrated throughout!")
print(
    f"  Running Python {sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}"
)

# Feature detection
import importlib.util

print(f"  Has tomllib: {importlib.util.find_spec('tomllib') is not None}")
print(
    f"  Has typing_extensions: {importlib.util.find_spec('typing_extensions') is not None}"
)


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ SECTION 37: FUNCTIONAL PROGRAMMING PATTERNS                            ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print("\n§37 — FUNCTIONAL PROGRAMMING PATTERNS")

from functools import reduce, partial
from operator import add, mul

# Map-reduce pipeline
numbers = list(range(1, 11))
pipeline_result = reduce(
    add, map(lambda x: x**2, filter(lambda x: x % 2 == 0, numbers))
)
print(f"  Map-filter-reduce: sum of even squares = {pipeline_result}")


# Currying via partial
def power(base, exponent):
    return base**exponent


square = partial(power, exponent=2)
cube = partial(power, exponent=3)
print(f"  Currying: square(5)={square(5)}, cube(3)={cube(3)}")


# Composition
def compose(*funcs):
    def composed(x):
        for f in reversed(funcs):
            x = f(x)
        return x

    return composed


double_then_add1 = compose(lambda x: x + 1, lambda x: x * 2)
print(f"  Compose: double_then_add1(5) = {double_then_add1(5)}")

# Immutable operations
original = (1, 2, 3, 4, 5)
transformed = tuple(x * 2 for x in original if x > 2)
print(f"  Immutable transform: {original} → {transformed}")


# ╔══════════════════════════════════════════════════════════════════════════╗
# ║ FINAL SUMMARY                                                           ║
# ╚══════════════════════════════════════════════════════════════════════════╝
print(f"\n{'=' * 70}")
print("SHOWCASE COMPLETE!")
print(f"{'=' * 70}")
print(f"""
Sections covered:
 §1  Basic Data Types & Variables     §20 Closures, Scoping & Advanced Funcs
 §2  Collections                      §21 Unpacking & Assignment Features
 §3  Operators                        §22 Iterators & Iteration Protocol
 §4  Control Flow                     §23 Memory & Performance
 §5  Functions                        §24 Standard Library Highlights
 §6  Comprehensions & Generators      §25 Structural Pattern Matching (Adv)
 §7  Decorators                       §26 Type System Advanced
 §8  Classes & OOP                    §27 Testing Patterns
 §9  Abstract Classes & Protocols     §28 Advanced Patterns & Idioms
 §10 Dataclasses, Enums, Typing       §29 Dunders Grab Bag
 §11 Exception Handling               §30 Misc Features
 §12 Context Managers                 §31 Bisect, Heapq & Data Structures
 §13 Itertools, Functools & Operator  §32 OS & System Interaction
 §14 String Features                  §33 Logging & Warnings
 §15 File I/O & Pathlib               §34 Struct, Bytes & Binary Data
 §16 Concurrency & Parallelism        §35 AST & Code Objects
 §17 Descriptors & Metaprogramming    §36 Version-Specific Features
 §18 Magic/Dunder Methods Deep Dive   §37 Functional Programming Patterns
 §19 Introspection & Reflection
""")
