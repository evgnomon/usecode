# Struct

A struct is a plain bundle of fields — state without behavior. Where a class pairs data with operations, a struct is just the data. The same minimal `Color` record across languages:

::::{tab-set}

:::{tab-item} Zig
```zig
const std = @import("std");

const Color = struct {
    r: u8,
    g: u8,
    b: u8,
};

pub fn main() !void {
    const c = Color{ .r = 255, .g = 128, .b = 0 };
    try std.io.getStdOut().writer().print("#{x:0>2}{x:0>2}{x:0>2}\n", .{ c.r, c.g, c.b });
}
```
:::

:::{tab-item} Go
```go
package main

import "fmt"

type Color struct {
	R, G, B uint8
}

func main() {
	c := Color{R: 255, G: 128, B: 0}
	fmt.Printf("#%02x%02x%02x\n", c.R, c.G, c.B)
}
```
:::

:::{tab-item} Python
```python
from dataclasses import dataclass


@dataclass(frozen=True)
class Color:
    r: int
    g: int
    b: int


c = Color(255, 128, 0)
print(f"#{c.r:02x}{c.g:02x}{c.b:02x}")
```
:::

:::{tab-item} Rust
```rust
struct Color {
    r: u8,
    g: u8,
    b: u8,
}

fn main() {
    let c = Color { r: 255, g: 128, b: 0 };
    println!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
}
```
:::

:::{tab-item} C
```c
#include <stdio.h>

typedef struct {
    unsigned char r;
    unsigned char g;
    unsigned char b;
} Color;

int main(void) {
    Color c = {.r = 255, .g = 128, .b = 0};
    printf("#%02x%02x%02x\n", c.r, c.g, c.b);
}
```
:::

:::{tab-item} C++
```cpp
#include <cstdio>

struct Color {
    unsigned char r;
    unsigned char g;
    unsigned char b;
};

int main() {
    Color c{255, 128, 0};
    std::printf("#%02x%02x%02x\n", c.r, c.g, c.b);
}
```
:::

:::{tab-item} C#
```csharp
using System;

public readonly record struct Color(byte R, byte G, byte B);

var c = new Color(255, 128, 0);
Console.WriteLine($"#{c.R:x2}{c.G:x2}{c.B:x2}");
```
:::

:::{tab-item} TypeScript
```typescript
interface Color {
  r: number;
  g: number;
  b: number;
}

const c: Color = { r: 255, g: 128, b: 0 };
console.log(`#${c.r.toString(16).padStart(2, "0")}${c.g.toString(16).padStart(2, "0")}${c.b.toString(16).padStart(2, "0")}`);
```
:::

:::{tab-item} JavaScript
```javascript
const c = { r: 255, g: 128, b: 0 };
const hex = (n) => n.toString(16).padStart(2, "0");
console.log(`#${hex(c.r)}${hex(c.g)}${hex(c.b)}`);
```
:::

:::{tab-item} Kotlin
```kotlin
data class Color(val r: Int, val g: Int, val b: Int)

fun main() {
    val c = Color(255, 128, 0)
    println("#%02x%02x%02x".format(c.r, c.g, c.b))
}
```
:::

:::{tab-item} Scala
```scala
final case class Color(r: Int, g: Int, b: Int)

@main def run(): Unit =
  val c = Color(255, 128, 0)
  println(f"#${c.r}%02x${c.g}%02x${c.b}%02x")
```
:::

:::{tab-item} Java
```java
public record Color(int r, int g, int b) {
    public static void main(String[] args) {
        Color c = new Color(255, 128, 0);
        System.out.printf("#%02x%02x%02x%n", c.r(), c.g(), c.b());
    }
}
```
:::

:::{tab-item} Bash
```bash
#!/usr/bin/env bash
# Bash has no structs. We approximate one with an associative array
# whose keys are the field names.

declare -A c=([r]=255 [g]=128 [b]=0)
printf "#%02x%02x%02x\n" "${c[r]}" "${c[g]}" "${c[b]}"
```
:::

::::

## Update

Structs are often treated as immutable — produce a new value with one field changed rather than mutating in place:

::::{tab-set}

:::{tab-item} Zig
```zig
const dimmed = Color{ .r = c.r / 2, .g = c.g, .b = c.b };
```
:::

:::{tab-item} Go
```go
dimmed := c
dimmed.R = c.R / 2
```
:::

:::{tab-item} Python
```python
from dataclasses import replace

dimmed = replace(c, r=c.r // 2)
```
:::

:::{tab-item} Rust
```rust
let dimmed = Color { r: c.r / 2, ..c };
```
:::

:::{tab-item} C
```c
Color dimmed = c;
dimmed.r = c.r / 2;
```
:::

:::{tab-item} C++
```cpp
Color dimmed = c;
dimmed.r = c.r / 2;
```
:::

:::{tab-item} C#
```csharp
var dimmed = c with { R = (byte)(c.R / 2) };
```
:::

:::{tab-item} TypeScript
```typescript
const dimmed: Color = { ...c, r: c.r / 2 };
```
:::

:::{tab-item} JavaScript
```javascript
const dimmed = { ...c, r: c.r / 2 };
```
:::

:::{tab-item} Kotlin
```kotlin
val dimmed = c.copy(r = c.r / 2)
```
:::

:::{tab-item} Scala
```scala
val dimmed = c.copy(r = c.r / 2)
```
:::

:::{tab-item} Java
```java
Color dimmed = new Color(c.r() / 2, c.g(), c.b());
```
:::

:::{tab-item} Bash
```bash
declare -A dimmed
for k in r g b; do dimmed[$k]="${c[$k]}"; done
dimmed[r]=$(( c[r] / 2 ))
```
:::

::::
