// ╔══════════════════════════════════════════════════════════════════════════════╗
// ║              THE COMPLETE TYPESCRIPT SHOWCASE — v5.x                       ║
// ║  Every major language feature & standard-library capability in one file.   ║
// ╚══════════════════════════════════════════════════════════════════════════════╝

// ============================================================================
// 1. PRIMITIVE TYPES
// ============================================================================

let isDone: boolean = false;
let decimal: number = 42;
let hex: number = 0xff;
let binary: number = 0b1010;
let octal: number = 0o744;
let big: bigint = 9007199254740991n;
let color: string = "blue";
let templateStr: string = `The answer is ${decimal}`;
let sym: symbol = Symbol("unique");
let uniqueSym: unique symbol = Symbol("singleton");
let nothing: null = null;
let notDefined: undefined = undefined;
let impossible: never = (() => { throw new Error(); })(); // only assignable to never

// ============================================================================
// 2. SPECIAL TYPES: any, unknown, void, never, object
// ============================================================================

let anything: any = 4;          // escape hatch — disables type checking
anything = "now a string";      // no error

let mystery: unknown = 42;      // safe counterpart to any
// mystery.toFixed();            // Error! must narrow first
if (typeof mystery === "number") {
  mystery.toFixed(2);            // OK after type guard
}

function logMessage(msg: string): void {   // void — no return value
  console.log(msg);
}

function fail(message: string): never {    // never — function never returns
  throw new Error(message);
}

function infiniteLoop(): never {
  while (true) {}
}

let obj: object = { key: "value" };        // non-primitive type

// ============================================================================
// 3. ARRAYS, TUPLES & READONLY ARRAYS
// ============================================================================

let numbers: number[] = [1, 2, 3];
let strings: Array<string> = ["a", "b", "c"]; // generic array syntax

// Tuples — fixed-length, typed-position arrays
let pair: [string, number] = ["age", 30];
let labeled: [name: string, age: number] = ["Alice", 25]; // labeled tuples

// Optional & rest elements in tuples
type Flexible = [string, number?, ...boolean[]];
const flex: Flexible = ["hello", 42, true, false, true];

// Readonly
let frozen: readonly number[] = [1, 2, 3];
// frozen.push(4);  // Error!
let frozenTuple: Readonly<[string, number]> = ["locked", 99];

// ============================================================================
// 4. ENUMS
// ============================================================================

// Numeric enum
enum Direction {
  Up,        // 0
  Down,      // 1
  Left,      // 2
  Right,     // 3
}

// String enum
enum LogLevel {
  Debug = "DEBUG",
  Info  = "INFO",
  Warn  = "WARN",
  Error = "ERROR",
}

// Heterogeneous enum (not recommended but possible)
enum Mixed {
  No = 0,
  Yes = "YES",
}

// Const enum — inlined at compile time, no runtime object
const enum Flags {
  Read    = 1 << 0,  // 1
  Write   = 1 << 1,  // 2
  Execute = 1 << 2,  // 4
}
let perm: number = Flags.Read | Flags.Write; // inlined to 3

// Enum as a type
function move(dir: Direction): void {
  console.log(`Moving ${Direction[dir]}`); // reverse mapping (numeric only)
}

// ============================================================================
// 5. INTERFACES
// ============================================================================

interface User {
  readonly id: number;           // immutable after creation
  name: string;
  email: string;
  age?: number;                  // optional property
  [meta: string]: unknown;       // index signature — allows extra keys
}

// Extending interfaces
interface Admin extends User {
  permissions: string[];
}

// Multiple inheritance
interface Serializable {
  toJSON(): string;
}

interface PersistableAdmin extends Admin, Serializable {
  lastLogin: Date;
}

// Call signatures & construct signatures
interface Greeter {
  (name: string): string;            // callable
  new (locale: string): Greeter;     // constructable
  defaultGreeting: string;           // property
}

// Hybrid types
interface Counter {
  (start: number): string;
  interval: number;
  reset(): void;
}

// ============================================================================
// 6. TYPE ALIASES & UNION / INTERSECTION TYPES
// ============================================================================

type StringOrNumber = string | number;              // union
type Point = { x: number; y: number };              // object alias
type Pair<T> = [T, T];                              // generic alias
type Nullable<T> = T | null | undefined;            // utility alias

// Intersection — combine types
type Timestamped = { createdAt: Date; updatedAt: Date };
type TimestampedUser = User & Timestamped;

// Discriminated unions — the backbone of safe state modeling
type Result<T, E = Error> =
  | { success: true; data: T }
  | { success: false; error: E };

function handleResult(result: Result<string>) {
  if (result.success) {
    console.log(result.data.toUpperCase());   // narrowed to { data: string }
  } else {
    console.error(result.error.message);      // narrowed to { error: Error }
  }
}

// ============================================================================
// 7. LITERAL TYPES & TEMPLATE LITERAL TYPES
// ============================================================================

type Yes = "yes";
type One = 1;
type True = true;

type Alignment = "left" | "center" | "right";
type Digit = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;
type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
type HttpStatus = 200 | 201 | 301 | 400 | 401 | 403 | 404 | 500;

// Template literal types — string manipulation at the type level
type EventName = `${"click" | "hover" | "focus"}Handler`;
// → "clickHandler" | "hoverHandler" | "focusHandler"

type Getters<T> = {
  [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};
type UserGetters = Getters<{ name: string; age: number }>;
// → { getName: () => string; getAge: () => number }

// Intrinsic string manipulation types
type Upper = Uppercase<"hello">;         // "HELLO"
type Lower = Lowercase<"HELLO">;         // "hello"
type Cap   = Capitalize<"hello">;        // "Hello"
type Uncap = Uncapitalize<"Hello">;      // "hello"

// ============================================================================
// 8. TYPE ASSERTIONS & CONST ASSERTIONS
// ============================================================================

let someValue: unknown = "hello world";
let strLength1: number = (someValue as string).length;     // as syntax
let strLength2: number = (<string>someValue).length;       // angle-bracket syntax

// const assertion — narrowest possible type
let mutable = { x: 10, y: 20 };                          // { x: number; y: number }
let immutable = { x: 10, y: 20 } as const;               // { readonly x: 10; readonly y: 20 }

const routes = ["home", "about", "contact"] as const;     // readonly ["home", "about", "contact"]
type Route = typeof routes[number];                       // "home" | "about" | "contact"

// satisfies — validate a type without widening
type Colors = Record<string, [number, number, number] | string>;
const palette = {
  red: [255, 0, 0],
  green: "#00ff00",
} satisfies Colors;
// palette.red is still [number, number, number], not widened to Colors value

// Non-null assertion
function getLength(s: string | null): number {
  return s!.length; // developer asserts s is non-null
}

// ============================================================================
// 9. FUNCTIONS — OVERLOADS, REST, GENERICS, THIS
// ============================================================================

// Basic typed function
function add(a: number, b: number): number {
  return a + b;
}

// Arrow function
const multiply = (a: number, b: number): number => a * b;

// Optional & default parameters
function greet(name: string, greeting: string = "Hello"): string {
  return `${greeting}, ${name}!`;
}

// Rest parameters
function sum(...nums: number[]): number {
  return nums.reduce((acc, n) => acc + n, 0);
}

// Function overloads
function format(value: string): string;
function format(value: number): string;
function format(value: string | number): string {
  if (typeof value === "string") return value.trim();
  return value.toFixed(2);
}

// Generic functions
function identity<T>(value: T): T {
  return value;
}
function firstElement<T>(arr: T[]): T | undefined {
  return arr[0];
}

// Constrained generics
function longest<T extends { length: number }>(a: T, b: T): T {
  return a.length >= b.length ? a : b;
}

// This parameter typing
interface Deck {
  suits: string[];
  createCardPicker(this: Deck): () => string;
}

// Function types
type MathOp = (a: number, b: number) => number;
type Predicate<T> = (item: T) => boolean;
type AsyncFn<T> = () => Promise<T>;
type Callback<T> = (err: Error | null, result?: T) => void;

// ============================================================================
// 10. GENERICS — ADVANCED
// ============================================================================

// Generic interfaces
interface Repository<T> {
  find(id: string): Promise<T>;
  findAll(): Promise<T[]>;
  save(entity: T): Promise<void>;
  delete(id: string): Promise<boolean>;
}

// Generic classes
class Stack<T> {
  private items: T[] = [];
  push(item: T): void { this.items.push(item); }
  pop(): T | undefined { return this.items.pop(); }
  peek(): T | undefined { return this.items[this.items.length - 1]; }
  get size(): number { return this.items.length; }
}

// Multiple type parameters
function zip<A, B>(as: A[], bs: B[]): [A, B][] {
  return as.map((a, i) => [a, bs[i]]);
}

// Generic constraints with keyof
function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
  return obj[key];
}

// Default type parameters
interface ApiResponse<T = unknown, E = Error> {
  data?: T;
  error?: E;
  status: number;
}

// Generic conditional defaults
type Container<T> = T extends string ? { text: T } : { value: T };
const textBox: Container<string> = { text: "hello" };
const numBox: Container<number> = { value: 42 };

// ============================================================================
// 11. CLASSES — FULL FEATURE SET
// ============================================================================

abstract class Shape {
  abstract area(): number;
  abstract perimeter(): number;

  describe(): string {
    return `Shape with area ${this.area().toFixed(2)}`;
  }
}

class Circle extends Shape {
  constructor(public readonly radius: number) {
    super();
  }
  area(): number { return Math.PI * this.radius ** 2; }
  perimeter(): number { return 2 * Math.PI * this.radius; }
}

class Rectangle extends Shape {
  // Parameter properties — shorthand for declaring & assigning
  constructor(
    private width: number,
    private height: number,
  ) {
    super();
  }
  area(): number { return this.width * this.height; }
  perimeter(): number { return 2 * (this.width + this.height); }
}

// Access modifiers: public, private, protected, readonly
class Employee {
  public name: string;
  private salary: number;
  protected department: string;
  readonly id: string;

  #secret: string = "hidden"; // ES private field (runtime enforcement)

  constructor(name: string, salary: number, department: string) {
    this.id = crypto.randomUUID();
    this.name = name;
    this.salary = salary;
    this.department = department;
  }

  // Getters & setters
  get annualSalary(): number {
    return this.salary * 12;
  }
  set monthlySalary(value: number) {
    this.salary = value;
  }

  // Static members
  static readonly MAX_NAME_LENGTH = 100;
  static validateName(name: string): boolean {
    return name.length <= Employee.MAX_NAME_LENGTH;
  }
}

// Implementing interfaces
interface Printable {
  print(): string;
}
interface Loggable {
  log(): void;
}

class Report implements Printable, Loggable {
  constructor(private title: string) {}
  print(): string { return `Report: ${this.title}`; }
  log(): void { console.log(this.print()); }
}

// Class expressions
const Animal = class {
  constructor(public name: string) {}
  speak(): string { return `${this.name} makes a sound`; }
};

// Abstract with generics
abstract class DataStore<T> {
  protected data: Map<string, T> = new Map();
  abstract serialize(item: T): string;
  abstract deserialize(raw: string): T;

  get(key: string): T | undefined {
    return this.data.get(key);
  }
  set(key: string, value: T): void {
    this.data.set(key, value);
  }
}

// ============================================================================
// 12. DECORATORS (Stage 3 — TC39 Standard)
// ============================================================================

// Class decorator
function sealed(constructor: Function) {
  Object.seal(constructor);
  Object.seal(constructor.prototype);
}

// Method decorator
function log(
  target: any,
  propertyKey: string,
  descriptor: PropertyDescriptor,
) {
  const original = descriptor.value;
  descriptor.value = function (...args: any[]) {
    console.log(`Calling ${propertyKey} with`, args);
    const result = original.apply(this, args);
    console.log(`${propertyKey} returned`, result);
    return result;
  };
  return descriptor;
}

// Property decorator
function defaultValue(value: any) {
  return function (target: any, propertyKey: string) {
    target[propertyKey] = value;
  };
}

// Parameter decorator
function required(target: any, propertyKey: string, parameterIndex: number) {
  const existingRequired: number[] =
    Reflect.getOwnMetadata("required", target, propertyKey) || [];
  existingRequired.push(parameterIndex);
  Reflect.defineMetadata("required", existingRequired, target, propertyKey);
}

// Applying decorators
@sealed
class GreeterClass {
  @defaultValue("World")
  greeting: string;

  @log
  greet(@required name: string): string {
    return `${this.greeting}, ${name}!`;
  }
}

// ============================================================================
// 13. TYPE NARROWING & TYPE GUARDS
// ============================================================================

// typeof guard
function padLeft(value: string, padding: string | number): string {
  if (typeof padding === "number") {
    return " ".repeat(padding) + value;
  }
  return padding + value;
}

// instanceof guard
function printShape(shape: Circle | Rectangle): void {
  if (shape instanceof Circle) {
    console.log(`Circle radius: ${shape.radius}`);
  } else {
    console.log(`Rectangle area: ${shape.area()}`);
  }
}

// in operator guard
interface Fish { swim(): void }
interface Bird { fly(): void }

function moveAnimal(animal: Fish | Bird) {
  if ("swim" in animal) {
    animal.swim();
  } else {
    animal.fly();
  }
}

// Custom type guard (type predicate)
function isString(value: unknown): value is string {
  return typeof value === "string";
}

// Assertion function
function assertDefined<T>(val: T | null | undefined, msg?: string): asserts val is T {
  if (val === null || val === undefined) {
    throw new Error(msg ?? "Value is null/undefined");
  }
}

// Discriminated union narrowing
type Shape2D =
  | { kind: "circle"; radius: number }
  | { kind: "rect"; width: number; height: number }
  | { kind: "triangle"; base: number; height: number };

function area(shape: Shape2D): number {
  switch (shape.kind) {
    case "circle":   return Math.PI * shape.radius ** 2;
    case "rect":     return shape.width * shape.height;
    case "triangle": return 0.5 * shape.base * shape.height;
    default: {
      const _exhaustive: never = shape;  // exhaustiveness check
      return _exhaustive;
    }
  }
}

// Truthiness narrowing
function printName(name: string | null | undefined) {
  if (name) {
    console.log(name.toUpperCase()); // narrowed to string
  }
}

// ============================================================================
// 14. CONDITIONAL TYPES
// ============================================================================

// Basic conditional
type IsString<T> = T extends string ? true : false;
type A = IsString<"hello">;   // true
type B = IsString<42>;        // false

// Distributive conditional types
type ToArray<T> = T extends any ? T[] : never;
type StrOrNumArray = ToArray<string | number>; // string[] | number[]

// Non-distributive (wrapped in tuple)
type ToArrayNonDist<T> = [T] extends [any] ? T[] : never;
type Mixed2 = ToArrayNonDist<string | number>; // (string | number)[]

// infer keyword — extract inner types
type UnpackPromise<T> = T extends Promise<infer U> ? U : T;
type Str = UnpackPromise<Promise<string>>;     // string
type Num = UnpackPromise<number>;              // number

type ReturnOf<T> = T extends (...args: any[]) => infer R ? R : never;
type Added = ReturnOf<typeof add>;             // number

type FirstArg<T> = T extends (first: infer F, ...rest: any[]) => any ? F : never;
type AddFirst = FirstArg<typeof add>;          // number

// Nested infer
type UnpackArray<T> = T extends Array<infer U>
  ? U extends Array<infer V>
    ? V
    : U
  : T;
type Deep = UnpackArray<number[][]>;           // number

// ============================================================================
// 15. MAPPED TYPES
// ============================================================================

// Basic mapped type
type Optional<T> = { [K in keyof T]?: T[K] };
type ReadOnly<T> = { readonly [K in keyof T]: T[K] };
type Mutable<T> = { -readonly [K in keyof T]: T[K] };
type Required2<T> = { [K in keyof T]-?: T[K] };

// Key remapping with `as`
type Prefixed<T, P extends string> = {
  [K in keyof T as `${P}${Capitalize<string & K>}`]: T[K];
};
type PrefixedUser = Prefixed<{ name: string; age: number }, "get">;
// → { getName: string; getAge: number }

// Filtering keys
type OnlyStrings<T> = {
  [K in keyof T as T[K] extends string ? K : never]: T[K];
};
type StringFields = OnlyStrings<{ name: string; age: number; email: string }>;
// → { name: string; email: string }

// Mapped type + conditional
type NullableProps<T> = {
  [K in keyof T]: T[K] | null;
};

// ============================================================================
// 16. UTILITY TYPES (Standard Library)
// ============================================================================

interface Todo {
  title: string;
  description: string;
  completed: boolean;
  priority: 1 | 2 | 3;
  tags: string[];
}

// Object utility types
type PartialTodo    = Partial<Todo>;                     // all optional
type RequiredTodo   = Required<PartialTodo>;             // all required again
type ReadonlyTodo   = Readonly<Todo>;                    // all readonly
type PickedTodo     = Pick<Todo, "title" | "completed">; // subset
type OmittedTodo    = Omit<Todo, "tags">;                // exclude keys
type RecordMap      = Record<string, number>;            // { [k: string]: number }

// Union utility types
type T1 = Exclude<"a" | "b" | "c", "a">;            // "b" | "c"
type T2 = Extract<"a" | "b" | "c", "a" | "d">;      // "a"
type T3 = NonNullable<string | null | undefined>;    // string

// Function utility types
type Params     = Parameters<typeof greet>;           // [name: string, greeting?: string]
type Return     = ReturnType<typeof greet>;           // string
type CtorParams = ConstructorParameters<typeof Employee>;
type Instance   = InstanceType<typeof Employee>;

// String utility types (built-in)
type U1 = Uppercase<"hello">;      // "HELLO"
type U2 = Lowercase<"HELLO">;      // "hello"
type U3 = Capitalize<"hello">;     // "Hello"
type U4 = Uncapitalize<"Hello">;   // "hello"

// Promise utility
type Awaited1 = Awaited<Promise<Promise<string>>>; // string

// ThisParameterType & OmitThisParameter
function toHex(this: number): string {
  return this.toString(16);
}
type HexThis = ThisParameterType<typeof toHex>; // number

// ============================================================================
// 17. MODULES & NAMESPACES
// ============================================================================

// Named exports
export const PI = 3.14159;
export function circleArea(r: number): number { return PI * r ** 2; }
export class Vector2D {
  constructor(public x: number, public y: number) {}
  magnitude(): number { return Math.sqrt(this.x ** 2 + this.y ** 2); }
}

// Default export (one per module)
export default class App {
  start(): void { console.log("App started"); }
}

// Re-exports
// export { User } from "./types";
// export { default as Config } from "./config";
// export * from "./utils";
// export * as helpers from "./helpers";

// Type-only imports/exports
export type { User, Admin };
// import type { User } from "./types";

// Import assertions (for JSON modules, etc.)
// import data from "./data.json" assert { type: "json" };

// Namespaces (legacy but still supported)
namespace Geometry {
  export interface Point { x: number; y: number }
  export function distance(a: Point, b: Point): number {
    return Math.sqrt((b.x - a.x) ** 2 + (b.y - a.y) ** 2);
  }
  export namespace ThreeD {
    export interface Point extends Geometry.Point { z: number }
  }
}

// Declaration merging — interfaces merge automatically
interface Box { height: number; width: number }
interface Box { depth: number }
// Box now has height, width, and depth

// ============================================================================
// 18. ASYNC / AWAIT & PROMISES
// ============================================================================

// Basic async function
async function fetchUser(id: string): Promise<User> {
  const response = await fetch(`/api/users/${id}`);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.json();
}

// Async generators
async function* paginate<T>(
  fetcher: (page: number) => Promise<T[]>,
): AsyncGenerator<T[], void, undefined> {
  let page = 0;
  let results: T[];
  do {
    results = await fetcher(page++);
    if (results.length > 0) yield results;
  } while (results.length > 0);
}

// Promise combinators
async function loadDashboard() {
  // All must succeed
  const [users, posts, comments] = await Promise.all([
    fetchUser("1"),
    fetch("/api/posts").then(r => r.json()),
    fetch("/api/comments").then(r => r.json()),
  ]);

  // First to settle (succeed or fail)
  const fastest = await Promise.race([
    fetch("/api/primary"),
    fetch("/api/fallback"),
  ]);

  // All settled — never rejects
  const results = await Promise.allSettled([
    fetch("/api/a"),
    fetch("/api/b"),
    fetch("/api/c"),
  ]);
  results.forEach(r => {
    if (r.status === "fulfilled") console.log(r.value);
    else console.error(r.reason);
  });

  // First successful — rejects only if all reject
  const winner = await Promise.any([
    fetch("/api/mirror1"),
    fetch("/api/mirror2"),
  ]);
}

// Async iteration (for-await-of)
async function processStream(stream: AsyncIterable<string>) {
  for await (const chunk of stream) {
    console.log(chunk);
  }
}

// ============================================================================
// 19. ITERATORS & GENERATORS
// ============================================================================

// Iterable interface
class Range implements Iterable<number> {
  constructor(
    private start: number,
    private end: number,
    private step: number = 1,
  ) {}

  [Symbol.iterator](): Iterator<number> {
    let current = this.start;
    const end = this.end;
    const step = this.step;
    return {
      next(): IteratorResult<number> {
        if (current <= end) {
          const value = current;
          current += step;
          return { value, done: false };
        }
        return { value: undefined, done: true };
      },
    };
  }
}

// For-of loop (works with iterables)
for (const n of new Range(1, 5)) {
  console.log(n); // 1, 2, 3, 4, 5
}

// Generator function
function* fibonacci(): Generator<number, void, undefined> {
  let a = 0, b = 1;
  while (true) {
    yield a;
    [a, b] = [b, a + b];
  }
}

// Generator with return and next input
function* accumulator(): Generator<number, string, number> {
  let total = 0;
  while (true) {
    const input: number = yield total;
    if (input < 0) return `Final: ${total}`;
    total += input;
  }
}

// Delegating generators
function* concat<T>(...iterables: Iterable<T>[]): Generator<T> {
  for (const it of iterables) {
    yield* it;
  }
}

// ============================================================================
// 20. SYMBOLS & WELL-KNOWN SYMBOLS
// ============================================================================

const uniqueKey = Symbol("description");
const obj2: { [uniqueKey]: string } = { [uniqueKey]: "value" };

// Well-known symbols
class CustomCollection {
  private items: number[] = [1, 2, 3];

  // Make iterable
  [Symbol.iterator](): Iterator<number> {
    let index = 0;
    const items = this.items;
    return {
      next(): IteratorResult<number> {
        return index < items.length
          ? { value: items[index++], done: false }
          : { value: undefined, done: true };
      },
    };
  }

  // Custom string tag
  get [Symbol.toStringTag](): string {
    return "CustomCollection";
  }

  // instanceof behavior
  static [Symbol.hasInstance](instance: any): boolean {
    return Array.isArray(instance?.items);
  }

  // Primitive conversion
  [Symbol.toPrimitive](hint: string): number | string {
    if (hint === "number") return this.items.length;
    return `Collection(${this.items.join(", ")})`;
  }
}

// ============================================================================
// 21. ADVANCED TYPE PATTERNS
// ============================================================================

// Recursive types
type JSONValue =
  | string
  | number
  | boolean
  | null
  | JSONValue[]
  | { [key: string]: JSONValue };

type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K];
};

type DeepReadonly<T> = {
  readonly [K in keyof T]: T[K] extends object ? DeepReadonly<T[K]> : T[K];
};

// Variadic tuple types
type Concat<A extends readonly any[], B extends readonly any[]> = [...A, ...B];
type AB = Concat<[1, 2], [3, 4]>; // [1, 2, 3, 4]

type Head<T extends any[]> = T extends [infer H, ...any[]] ? H : never;
type Tail<T extends any[]> = T extends [any, ...infer R] ? R : [];
type Last<T extends any[]> = T extends [...any[], infer L] ? L : never;

// Branded / opaque types
type Brand<T, B extends string> = T & { readonly __brand: B };
type USD = Brand<number, "USD">;
type EUR = Brand<number, "EUR">;
type UserID = Brand<string, "UserID">;
type OrderID = Brand<string, "OrderID">;

function createUSD(amount: number): USD { return amount as USD; }
function createEUR(amount: number): EUR { return amount as EUR; }
// const total: USD = createUSD(10) + createEUR(5); // Error! Can't mix brands

// Builder pattern with generics
type BuilderState = {
  host: boolean;
  port: boolean;
  protocol: boolean;
};

class URLBuilder<State extends Partial<BuilderState> = {}> {
  private _host = "";
  private _port = 80;
  private _protocol = "https";

  host(h: string): URLBuilder<State & { host: true }> {
    this._host = h;
    return this as any;
  }
  port(p: number): URLBuilder<State & { port: true }> {
    this._port = p;
    return this as any;
  }
  protocol(p: string): URLBuilder<State & { protocol: true }> {
    this._protocol = p;
    return this as any;
  }
  // build() is only available when all required fields are set
  build(this: URLBuilder<{ host: true; port: true; protocol: true }>): string {
    return `${this._protocol}://${this._host}:${this._port}`;
  }
}

// ============================================================================
// 22. INDEXED ACCESS TYPES & keyof / typeof
// ============================================================================

type PersonAge = User["age"];                           // number | undefined
type UserKeys = keyof User;                             // "id" | "name" | "email" | "age"

const config = {
  api: { url: "https://api.example.com", timeout: 5000 },
  db: { host: "localhost", port: 5432 },
} as const;

type Config = typeof config;
type ApiConfig = typeof config.api;
type DbHost = typeof config.db.host;                    // "localhost" (literal)

// Indexed access with unions
type ResponseKeys = keyof ApiResponse;
type ResponseValues = ApiResponse[keyof ApiResponse];

// Recursive indexed access
type NestedPaths<T, Prefix extends string = ""> = {
  [K in keyof T]: T[K] extends object
    ? NestedPaths<T[K], `${Prefix}${string & K}.`>
    : `${Prefix}${string & K}`;
}[keyof T];

// ============================================================================
// 23. DECLARATION FILES & AMBIENT TYPES
// ============================================================================

// Ambient declarations (typically in .d.ts files)
declare const GLOBAL_VERSION: string;
declare function externalLib(input: string): number;

declare module "some-untyped-lib" {
  export function doThing(x: number): string;
  export const version: string;
}

// Module augmentation
declare module "./types" {
  interface User {
    lastSeen?: Date;
  }
}

// Global augmentation
declare global {
  interface Window {
    analytics: {
      track(event: string, props?: Record<string, unknown>): void;
    };
  }
  // Add to Array prototype
  interface Array<T> {
    customShuffle(): T[];
  }
}

// Triple-slash directives (legacy but still used)
/// <reference types="node" />
/// <reference path="./global.d.ts" />

// ============================================================================
// 24. ERROR HANDLING PATTERNS
// ============================================================================

// Custom error classes
class AppError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly statusCode: number = 500,
    public readonly cause?: Error,
  ) {
    super(message);
    this.name = "AppError";
    // Fix prototype chain
    Object.setPrototypeOf(this, AppError.prototype);
  }
}

class NotFoundError extends AppError {
  constructor(resource: string, id: string) {
    super(`${resource} with id '${id}' not found`, "NOT_FOUND", 404);
    this.name = "NotFoundError";
  }
}

class ValidationError extends AppError {
  constructor(
    public readonly field: string,
    public readonly constraint: string,
  ) {
    super(`Validation failed: ${field} ${constraint}`, "VALIDATION", 400);
    this.name = "ValidationError";
  }
}

// Type-safe error handling with Result type
type ResultOk<T> = { ok: true; value: T };
type ResultErr<E> = { ok: false; error: E };
type SafeResult<T, E = Error> = ResultOk<T> | ResultErr<E>;

function trySafe<T>(fn: () => T): SafeResult<T> {
  try {
    return { ok: true, value: fn() };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e : new Error(String(e)) };
  }
}

async function trySafeAsync<T>(fn: () => Promise<T>): Promise<SafeResult<T>> {
  try {
    return { ok: true, value: await fn() };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e : new Error(String(e)) };
  }
}

// Error cause chaining (ES2022)
function processFile(path: string) {
  try {
    // ... read file
  } catch (err) {
    throw new Error(`Failed to process ${path}`, { cause: err });
  }
}

// ============================================================================
// 25. ES2015+ FEATURES FULLY TYPED
// ============================================================================

// --- Destructuring ---
const { name: userName, age: userAge = 0 } = { name: "Alice", age: 30 };
const [first, second, ...rest] = [1, 2, 3, 4, 5];
const { api: { url: apiUrl } } = config; // nested destructuring

// --- Spread ---
const merged = { ...config.api, ...config.db };
const extended = [...numbers, 4, 5, 6];

// --- Optional Chaining & Nullish Coalescing ---
const user: User | null = null;
const emailLength = user?.email?.length;          // number | undefined
const displayName = user?.name ?? "Anonymous";    // string

// Nullish coalescing assignment
let setting: string | null = null;
setting ??= "default"; // assigns only if null/undefined

// Logical assignment operators
let count = 0;
count ||= 10;   // assigns if falsy
count &&= 20;   // assigns if truthy

// --- Map, Set, WeakMap, WeakSet ---
const userMap = new Map<string, User>();
userMap.set("1", { id: 1, name: "Alice", email: "a@b.com" });
userMap.set("2", { id: 2, name: "Bob", email: "b@b.com" });

const uniqueIds = new Set<number>([1, 2, 3, 2, 1]); // {1, 2, 3}
const weakCache = new WeakMap<object, string>();
const weakSet = new WeakSet<object>();

// --- Proxy & Reflect ---
interface DataObject { [key: string]: any }

const handler: ProxyHandler<DataObject> = {
  get(target, prop, receiver) {
    console.log(`Accessing ${String(prop)}`);
    return Reflect.get(target, prop, receiver);
  },
  set(target, prop, value, receiver) {
    console.log(`Setting ${String(prop)} = ${value}`);
    return Reflect.set(target, prop, value, receiver);
  },
  has(target, prop) {
    console.log(`Checking if ${String(prop)} exists`);
    return Reflect.has(target, prop);
  },
  deleteProperty(target, prop) {
    console.log(`Deleting ${String(prop)}`);
    return Reflect.deleteProperty(target, prop);
  },
};

const observed = new Proxy<DataObject>({}, handler);

// --- WeakRef & FinalizationRegistry ---
let heavyObject: { data: number[] } | null = { data: new Array(1000).fill(0) };
const weakRef = new WeakRef(heavyObject);
heavyObject = null; // eligible for GC

const registry = new FinalizationRegistry((heldValue: string) => {
  console.log(`${heldValue} was garbage collected`);
});
const trackedObj = { important: true };
registry.register(trackedObj, "trackedObj");

// --- Intl (Internationalization) ---
const dateFormatter = new Intl.DateTimeFormat("en-US", {
  year: "numeric", month: "long", day: "numeric",
  hour: "2-digit", minute: "2-digit",
});
const numberFormatter = new Intl.NumberFormat("de-DE", {
  style: "currency", currency: "EUR",
});
const listFormatter = new Intl.ListFormat("en", {
  style: "long", type: "conjunction",
});
const relativeTime = new Intl.RelativeTimeFormat("en", { numeric: "auto" });
const pluralRules = new Intl.PluralRules("en-US");
const segmenter = new Intl.Segmenter("en", { granularity: "word" });
const collator = new Intl.Collator("sv", { sensitivity: "base" }); // Swedish sorting

// --- Temporal-like Date manipulation (using Date) ---
const now = new Date();
const utcString = now.toISOString();
const localString = now.toLocaleString("sv-SE");

// --- structuredClone (deep copy) ---
const original = { nested: { arr: [1, 2, 3], date: new Date() } };
const deepCopy = structuredClone(original);

// ============================================================================
// 26. PATTERN MATCHING & ADVANCED CONTROL FLOW
// ============================================================================

// Using 'using' declarations (Explicit Resource Management — ES2024/TS 5.2+)
// Symbol.dispose and Symbol.asyncDispose

interface Disposable {
  [Symbol.dispose](): void;
}

class TempFile implements Disposable {
  constructor(public path: string) {
    console.log(`Created temp file: ${path}`);
  }
  [Symbol.dispose](): void {
    console.log(`Deleted temp file: ${this.path}`);
  }
}

// using keyword ensures cleanup
async function processWithCleanup() {
  using file = new TempFile("/tmp/data.txt");
  // ... use file
  // file[Symbol.dispose]() called automatically at end of scope
}

// await using — for async disposal
interface AsyncDisposableResource {
  [Symbol.asyncDispose](): Promise<void>;
}

class DatabaseConnection implements AsyncDisposableResource {
  async [Symbol.asyncDispose](): Promise<void> {
    console.log("Closing database connection");
  }
}

// ============================================================================
// 27. ADVANCED GENERICS PATTERNS
// ============================================================================

// Higher-kinded type emulation
interface Functor<F> {
  map<A, B>(fa: F, f: (a: A) => B): F;
}

// Type-level arithmetic (recursive)
type BuildTuple<N extends number, T extends any[] = []> =
  T["length"] extends N ? T : BuildTuple<N, [...T, unknown]>;

type Add<A extends number, B extends number> =
  [...BuildTuple<A>, ...BuildTuple<B>]["length"];

type Five = Add<2, 3>; // 5

// Curried function type
type Curry<F> = F extends (...args: infer A) => infer R
  ? A extends [infer First, ...infer Rest]
    ? (arg: First) => Curry<(...args: Rest) => R>
    : R
  : never;

declare function curry<F extends (...args: any[]) => any>(fn: F): Curry<F>;

// Type-safe event emitter
type EventMap = {
  userLogin: { userId: string; timestamp: number };
  userLogout: { userId: string };
  error: { code: number; message: string };
  [key: `custom:${string}`]: Record<string, unknown>;
};

class TypedEventEmitter<Events extends Record<string, any>> {
  private handlers = new Map<keyof Events, Set<Function>>();

  on<K extends keyof Events>(event: K, handler: (data: Events[K]) => void): this {
    if (!this.handlers.has(event)) this.handlers.set(event, new Set());
    this.handlers.get(event)!.add(handler);
    return this;
  }

  emit<K extends keyof Events>(event: K, data: Events[K]): void {
    this.handlers.get(event)?.forEach(h => h(data));
  }

  off<K extends keyof Events>(event: K, handler: (data: Events[K]) => void): this {
    this.handlers.get(event)?.delete(handler);
    return this;
  }
}

const emitter = new TypedEventEmitter<EventMap>();
emitter.on("userLogin", ({ userId, timestamp }) => {
  console.log(`${userId} logged in at ${timestamp}`);
});

// Type-safe pipe / compose
function pipe<A>(value: A): A;
function pipe<A, B>(value: A, fn1: (a: A) => B): B;
function pipe<A, B, C>(value: A, fn1: (a: A) => B, fn2: (b: B) => C): C;
function pipe<A, B, C, D>(
  value: A, fn1: (a: A) => B, fn2: (b: B) => C, fn3: (c: C) => D
): D;
function pipe(value: any, ...fns: Function[]) {
  return fns.reduce((v, fn) => fn(v), value);
}

const result = pipe(
  "  Hello, World!  ",
  (s: string) => s.trim(),
  (s: string) => s.toLowerCase(),
  (s: string) => s.split(", "),
);

// ============================================================================
// 28. STANDARD BUILT-IN OBJECTS — TYPED
// ============================================================================

// --- Array methods (fully generic) ---
const nums = [3, 1, 4, 1, 5, 9, 2, 6, 5];
const sorted     = nums.toSorted((a, b) => a - b);        // non-mutating sort
const reversed   = nums.toReversed();                      // non-mutating reverse
const spliced    = nums.toSpliced(2, 1, 99);               // non-mutating splice
const replaced   = nums.with(0, 100);                      // non-mutating index set
const grouped    = Object.groupBy(nums, n => n % 2 === 0 ? "even" : "odd");
const found      = nums.find(n => n > 4);                  // number | undefined
const foundIndex = nums.findIndex(n => n > 4);
const foundLast  = nums.findLast(n => n > 4);              // ES2023
const flat       = [[1, 2], [3, [4, 5]]].flat(2);          // number[]
const mapped     = nums.flatMap(n => [n, n * 2]);

// --- Object methods ---
const entries: [string, number][] = Object.entries({ a: 1, b: 2 });
const fromEntries = Object.fromEntries(entries);
const keys: string[] = Object.keys({ a: 1, b: 2 });
const values: number[] = Object.values({ a: 1, b: 2 });
const hasOwn: boolean = Object.hasOwn({ a: 1 }, "a");     // ES2022

// --- String methods ---
const padded    = "42".padStart(5, "0");      // "00042"
const trimmed   = "  hello  ".trimStart();     // "hello  "
const replaced2 = "aabbcc".replaceAll("b", "x");
const atChar    = "hello".at(-1);              // "o"
const matches   = [..."hello".matchAll(/l/g)]; // IterableIterator

// --- RegExp ---
const namedGroup = /(?<year>\d{4})-(?<month>\d{2})/.exec("2026-03");
const year = namedGroup?.groups?.year;         // string | undefined

// Named capture with flags
const regex = /(?<word>\w+)/dg;                // 'd' flag = indices
const flagMatch = regex.exec("hello world");
const indices = flagMatch?.indices;            // start/end positions

// --- Math ---
const clamped = Math.max(0, Math.min(100, -5));  // manual clamp: 0
const truncated = Math.trunc(-4.7);               // -4

// --- Error types ---
const errors = [
  new TypeError("wrong type"),
  new RangeError("out of range"),
  new SyntaxError("bad syntax"),
  new ReferenceError("not defined"),
  new URIError("bad URI"),
  new EvalError("eval error"),
  new AggregateError([new Error("1"), new Error("2")], "multiple errors"),
];

// ============================================================================
// 29. TYPING PATTERNS FOR DOM & WEB APIs
// ============================================================================

// DOM types
function setupUI(): void {
  const canvas = document.querySelector("canvas"); // HTMLCanvasElement | null
  const input = document.getElementById("search") as HTMLInputElement;
  const buttons = document.querySelectorAll<HTMLButtonElement>(".btn");

  // Event typing
  document.addEventListener("click", (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    console.log(target.tagName, e.clientX, e.clientY);
  });

  document.addEventListener("keydown", (e: KeyboardEvent) => {
    if (e.key === "Escape") console.log("Escaped!");
  });

  // Fetch API typing
  async function fetchJSON<T>(url: string, init?: RequestInit): Promise<T> {
    const response = await fetch(url, {
      ...init,
      headers: { "Content-Type": "application/json", ...init?.headers },
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return response.json() as Promise<T>;
  }

  // AbortController
  const controller = new AbortController();
  fetch("/api/data", { signal: controller.signal }).catch(() => {});
  setTimeout(() => controller.abort(), 5000);

  // URL & URLSearchParams
  const url = new URL("https://example.com/search");
  url.searchParams.set("q", "typescript");
  url.searchParams.append("lang", "en");

  // FormData
  const form = document.querySelector("form");
  if (form) {
    const formData = new FormData(form);
    const entries2 = Object.fromEntries(formData.entries());
  }

  // IntersectionObserver
  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach(entry => {
        if (entry.isIntersecting) {
          entry.target.classList.add("visible");
        }
      });
    },
    { threshold: 0.1 },
  );

  // ResizeObserver
  const resizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      const { width, height } = entry.contentRect;
      console.log(`${width}x${height}`);
    }
  });

  // MutationObserver
  const mutationObserver = new MutationObserver((mutations) => {
    mutations.forEach(m => console.log(m.type, m.target));
  });

  // Web Workers typing
  const worker = new Worker("worker.js");
  worker.postMessage({ type: "start", data: [1, 2, 3] });
  worker.onmessage = (e: MessageEvent<{ result: number }>) => {
    console.log(e.data.result);
  };

  // BroadcastChannel
  const channel = new BroadcastChannel("app_events");
  channel.postMessage({ type: "sync" });

  // Clipboard API
  navigator.clipboard.writeText("copied!").then(() => console.log("Copied"));

  // Performance API
  performance.mark("start");
  performance.mark("end");
  performance.measure("duration", "start", "end");
  const measures = performance.getEntriesByType("measure");
}

// ============================================================================
// 30. TYPED DATA STRUCTURES
// ============================================================================

// --- Typed Map with utility methods ---
class TypedMap<K, V> {
  private map = new Map<K, V>();

  set(key: K, value: V): this { this.map.set(key, value); return this; }
  get(key: K): V | undefined { return this.map.get(key); }
  has(key: K): boolean { return this.map.has(key); }
  delete(key: K): boolean { return this.map.delete(key); }

  getOrDefault(key: K, defaultValue: V): V {
    return this.map.get(key) ?? defaultValue;
  }

  mapValues<U>(fn: (value: V, key: K) => U): TypedMap<K, U> {
    const result = new TypedMap<K, U>();
    this.map.forEach((v, k) => result.set(k, fn(v, k)));
    return result;
  }

  filter(predicate: (value: V, key: K) => boolean): TypedMap<K, V> {
    const result = new TypedMap<K, V>();
    this.map.forEach((v, k) => { if (predicate(v, k)) result.set(k, v); });
    return result;
  }

  toArray(): [K, V][] {
    return [...this.map.entries()];
  }
}

// --- Typed LinkedList ---
class ListNode<T> {
  constructor(
    public value: T,
    public next: ListNode<T> | null = null,
  ) {}
}

class LinkedList<T> implements Iterable<T> {
  private head: ListNode<T> | null = null;
  private _size = 0;

  get size(): number { return this._size; }

  prepend(value: T): this {
    this.head = new ListNode(value, this.head);
    this._size++;
    return this;
  }

  find(predicate: (value: T) => boolean): T | undefined {
    let current = this.head;
    while (current) {
      if (predicate(current.value)) return current.value;
      current = current.next;
    }
    return undefined;
  }

  *[Symbol.iterator](): Iterator<T> {
    let current = this.head;
    while (current) {
      yield current.value;
      current = current.next;
    }
  }

  toArray(): T[] {
    return [...this];
  }
}

// ============================================================================
// 31. DESIGN PATTERNS (Typed)
// ============================================================================

// --- Singleton ---
class Database {
  private static instance: Database;
  private constructor(private connectionString: string) {}

  static getInstance(connStr: string = "default"): Database {
    if (!Database.instance) {
      Database.instance = new Database(connStr);
    }
    return Database.instance;
  }
}

// --- Observer ---
interface Observer<T> {
  update(data: T): void;
}

class Subject<T> {
  private observers = new Set<Observer<T>>();
  subscribe(observer: Observer<T>): () => void {
    this.observers.add(observer);
    return () => this.observers.delete(observer);
  }
  notify(data: T): void {
    this.observers.forEach(o => o.update(data));
  }
}

// --- Strategy ---
interface SortStrategy<T> {
  sort(data: T[]): T[];
}

class QuickSort<T> implements SortStrategy<T> {
  sort(data: T[]): T[] {
    if (data.length <= 1) return data;
    const pivot = data[0];
    const left = data.slice(1).filter(x => x <= pivot);
    const right = data.slice(1).filter(x => x > pivot);
    return [...this.sort(left), pivot, ...this.sort(right)];
  }
}

class Sorter<T> {
  constructor(private strategy: SortStrategy<T>) {}
  setStrategy(strategy: SortStrategy<T>) { this.strategy = strategy; }
  sort(data: T[]): T[] { return this.strategy.sort(data); }
}

// --- State Machine ---
type State = "idle" | "loading" | "success" | "error";
type Event = "FETCH" | "RESOLVE" | "REJECT" | "RESET";

type Transitions = {
  [S in State]: {
    [E in Event]?: State;
  };
};

const transitions: Transitions = {
  idle:    { FETCH: "loading" },
  loading: { RESOLVE: "success", REJECT: "error" },
  success: { RESET: "idle", FETCH: "loading" },
  error:   { RESET: "idle", FETCH: "loading" },
};

class StateMachine {
  private current: State = "idle";

  transition(event: Event): State {
    const next = transitions[this.current][event];
    if (!next) throw new Error(`Invalid transition: ${this.current} + ${event}`);
    this.current = next;
    return this.current;
  }

  get state(): State { return this.current; }
}

// ============================================================================
// 32. ADVANCED UTILITY TYPE RECIPES
// ============================================================================

// Paths of an object (dot notation)
type Paths<T, Prefix extends string = ""> = T extends object
  ? {
      [K in keyof T]: K extends string
        ? T[K] extends object
          ? Paths<T[K], `${Prefix}${K}.`> | `${Prefix}${K}`
          : `${Prefix}${K}`
        : never;
    }[keyof T]
  : never;

type UserPaths = Paths<{ name: string; address: { city: string; zip: number } }>;
// → "name" | "address" | "address.city" | "address.zip"

// DeepPick
type DeepPick<T, P extends string> = P extends `${infer Key}.${infer Rest}`
  ? Key extends keyof T
    ? { [K in Key]: DeepPick<T[K], Rest> }
    : never
  : P extends keyof T
    ? { [K in P]: T[K] }
    : never;

// Mutable / Immutable deep
type DeepMutable<T> = {
  -readonly [K in keyof T]: T[K] extends object ? DeepMutable<T[K]> : T[K];
};

// Strict Omit (errors on invalid keys)
type StrictOmit<T, K extends keyof T> = Omit<T, K>;

// RequireAtLeastOne
type RequireAtLeastOne<T, Keys extends keyof T = keyof T> =
  Pick<T, Exclude<keyof T, Keys>> &
  { [K in Keys]-?: Required<Pick<T, K>> & Partial<Pick<T, Exclude<Keys, K>>> }[Keys];

type SearchParams = RequireAtLeastOne<{
  name?: string;
  email?: string;
  id?: string;
}>;
// Must provide at least one of name, email, or id

// RequireExactlyOne
type RequireExactlyOne<T, Keys extends keyof T = keyof T> =
  Pick<T, Exclude<keyof T, Keys>> &
  { [K in Keys]-?:
    Required<Pick<T, K>> &
    Partial<Record<Exclude<Keys, K>, never>>
  }[Keys];

// XOR type
type XOR<A, B> =
  | (A & { [K in keyof B]?: never })
  | (B & { [K in keyof A]?: never });

type PaymentMethod = XOR<
  { creditCard: string },
  { bankAccount: string }
>;

// ============================================================================
// 33. COMPILER / TSCONFIG FEATURES REFERENCE
// ============================================================================

/*
  Key tsconfig.json compiler options (not runnable, reference only):

  {
    "compilerOptions": {
      // --- Target & Module ---
      "target": "ES2023",              // JS version output
      "module": "NodeNext",            // module system
      "moduleResolution": "NodeNext",  // how imports resolve
      "lib": ["ES2023", "DOM"],        // available type libraries

      // --- Strictness ---
      "strict": true,                  // enables ALL strict checks:
        // "strictNullChecks": true,
        // "strictFunctionTypes": true,
        // "strictBindCallApply": true,
        // "strictPropertyInitialization": true,
        // "noImplicitAny": true,
        // "noImplicitThis": true,
        // "alwaysStrict": true,
        // "useUnknownInCatchVariables": true,

      // --- Additional Checks ---
      "noUnusedLocals": true,
      "noUnusedParameters": true,
      "noImplicitReturns": true,
      "noFallthroughCasesInSwitch": true,
      "noUncheckedIndexedAccess": true,  // T | undefined for index access
      "exactOptionalPropertyTypes": true,
      "noPropertyAccessFromIndexSignature": true,

      // --- Emit ---
      "outDir": "./dist",
      "rootDir": "./src",
      "declaration": true,             // generate .d.ts
      "declarationMap": true,
      "sourceMap": true,
      "inlineSources": true,
      "removeComments": false,
      "esModuleInterop": true,
      "allowSyntheticDefaultImports": true,
      "resolveJsonModule": true,
      "isolatedModules": true,         // safe for tools like esbuild
      "verbatimModuleSyntax": true,    // enforce import type

      // --- Decorators ---
      "experimentalDecorators": true,
      "emitDecoratorMetadata": true,

      // --- Path Mapping ---
      "baseUrl": ".",
      "paths": {
        "@/*": ["src/*"],
        "@components/*": ["src/components/*"]
      }
    },
    "include": ["src/**/*"],
    "exclude": ["node_modules", "dist"]
  }
*/

// ============================================================================
// 34. TYPE-SAFE FETCH WRAPPER (Practical Example)
// ============================================================================

// Typed HTTP client combining many features
type HttpMethod2 = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

interface ApiEndpoints {
  "/users":          { GET: User[]; POST: User };
  "/users/:id":      { GET: User; PUT: User; DELETE: void };
  "/posts":          { GET: { title: string; body: string }[] };
}

type ExtractParams<T extends string> =
  T extends `${infer _Start}:${infer Param}/${infer Rest}`
    ? { [K in Param | keyof ExtractParams<Rest>]: string }
    : T extends `${infer _Start}:${infer Param}`
      ? { [K in Param]: string }
      : {};

type EndpointResponse<
  Path extends keyof ApiEndpoints,
  Method extends keyof ApiEndpoints[Path],
> = ApiEndpoints[Path][Method];

async function apiCall<
  Path extends keyof ApiEndpoints,
  Method extends keyof ApiEndpoints[Path] & HttpMethod2,
>(
  path: Path,
  method: Method,
  ..._params: keyof ExtractParams<Path & string> extends never
    ? []
    : [params: ExtractParams<Path & string>]
): Promise<EndpointResponse<Path, Method>> {
  const response = await fetch(String(path), { method });
  return response.json();
}

// Usage — fully type-safe:
// const users = await apiCall("/users", "GET");           // User[]
// const user  = await apiCall("/users/:id", "GET", { id: "123" }); // User

// ============================================================================
// 35. FINAL SHOWCASE — PUTTING IT ALL TOGETHER
// ============================================================================

// A mini type-safe dependency injection container
type Constructor<T = any> = new (...args: any[]) => T;
type Token<T> = Constructor<T> | string | symbol;

class DIContainer {
  private bindings = new Map<Token<any>, () => any>();
  private singletons = new Map<Token<any>, any>();

  bind<T>(token: Token<T>, factory: () => T): this {
    this.bindings.set(token, factory);
    return this;
  }

  singleton<T>(token: Token<T>, factory: () => T): this {
    this.bindings.set(token, () => {
      if (!this.singletons.has(token)) {
        this.singletons.set(token, factory());
      }
      return this.singletons.get(token)!;
    });
    return this;
  }

  resolve<T>(token: Token<T>): T {
    const factory = this.bindings.get(token);
    if (!factory) throw new Error(`No binding for ${String(token)}`);
    return factory() as T;
  }
}

// Usage
const container = new DIContainer();
const DB_TOKEN = Symbol("Database");

container
  .singleton(DB_TOKEN, () => Database.getInstance("postgres://localhost/app"))
  .bind(Stack, () => new Stack<number>());

const db = container.resolve<Database>(DB_TOKEN);
const stack = container.resolve(Stack);

// ============================================================================
// EOF — This file demonstrates:
//   • All primitive & special types
//   • Arrays, tuples, readonly variants
//   • Enums (numeric, string, const)
//   • Interfaces & type aliases
//   • Union, intersection & discriminated unions
//   • Literal & template literal types
//   • Type assertions & const assertions (satisfies)
//   • Functions (overloads, rest, generics, this)
//   • Advanced generics (constraints, defaults, conditional, mapped)
//   • Classes (abstract, access, decorators, static, getters/setters)
//   • Decorators (class, method, property, parameter)
//   • Type narrowing & type guards (typeof, instanceof, in, predicates)
//   • Conditional types & infer
//   • Mapped types & key remapping
//   • All built-in utility types
//   • Modules, namespaces & declaration merging
//   • Async/await, generators, async generators
//   • Iterators & Symbol.iterator
//   • Symbols & well-known symbols
//   • Recursive types, branded types, variadic tuples
//   • keyof, typeof, indexed access types
//   • Ambient declarations & .d.ts patterns
//   • Error handling patterns (Result type, custom errors)
//   • ES2015–2024 features (destructuring, proxy, WeakRef, etc.)
//   • Explicit Resource Management (using / Symbol.dispose)
//   • Standard library: Array, Object, String, RegExp, Math, Intl
//   • DOM & Web API typing
//   • Typed data structures (Map, LinkedList)
//   • Design patterns (Singleton, Observer, Strategy, State Machine)
//   • Advanced utility type recipes
//   • tsconfig.json reference
//   • Practical typed HTTP client
//   • Dependency injection container
// ============================================================================
