# Class

A class bundles state and the operations on it. The same minimal `Point` type across languages:

::::{tab-set}

:::{tab-item} Zig
```zig
const std = @import("std");

const Point = struct {
    x: f64,
    y: f64,

    pub fn init(x: f64, y: f64) Point {
        return .{ .x = x, .y = y };
    }

    pub fn distanceTo(self: Point, other: Point) f64 {
        return std.math.hypot(self.x - other.x, self.y - other.y);
    }
};

pub fn main() !void {
    const p = Point.init(1.0, 2.0);
    const d = p.distanceTo(Point.init(4.0, 6.0));
    try std.io.getStdOut().writer().print("{d}\n", .{d});
}
```
:::

:::{tab-item} Go
```go
package main

import (
	"fmt"
	"math"
)

type Point struct {
	X, Y float64
}

func NewPoint(x, y float64) Point {
	return Point{X: x, Y: y}
}

func (p Point) DistanceTo(other Point) float64 {
	return math.Hypot(p.X-other.X, p.Y-other.Y)
}

func main() {
	p := NewPoint(1, 2)
	fmt.Println(p.DistanceTo(NewPoint(4, 6)))
}
```
:::

:::{tab-item} Python
```python
import math
from dataclasses import dataclass


@dataclass
class Point:
    x: float
    y: float

    def distance_to(self, other: "Point") -> float:
        return math.hypot(self.x - other.x, self.y - other.y)


p = Point(1.0, 2.0)
print(p.distance_to(Point(4.0, 6.0)))
```
:::

:::{tab-item} Rust
```rust
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn distance_to(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

fn main() {
    let p = Point::new(1.0, 2.0);
    println!("{}", p.distance_to(&Point::new(4.0, 6.0)));
}
```
:::

:::{tab-item} C
```c
#include <math.h>
#include <stdio.h>

typedef struct {
    double x;
    double y;
} Point;

static Point point_new(double x, double y) {
    return (Point){.x = x, .y = y};
}

static double point_distance_to(Point a, Point b) {
    return hypot(a.x - b.x, a.y - b.y);
}

int main(void) {
    Point p = point_new(1.0, 2.0);
    printf("%f\n", point_distance_to(p, point_new(4.0, 6.0)));
}
```
:::

:::{tab-item} C++
```cpp
#include <cmath>
#include <iostream>

class Point {
public:
    Point(double x, double y) : x_(x), y_(y) {}

    double distance_to(const Point& other) const {
        return std::hypot(x_ - other.x_, y_ - other.y_);
    }

private:
    double x_;
    double y_;
};

int main() {
    Point p(1.0, 2.0);
    std::cout << p.distance_to(Point(4.0, 6.0)) << '\n';
}
```
:::

:::{tab-item} C#
```csharp
using System;

public sealed class Point
{
    public double X { get; }
    public double Y { get; }

    public Point(double x, double y)
    {
        X = x;
        Y = y;
    }

    public double DistanceTo(Point other) =>
        Math.Sqrt(Math.Pow(X - other.X, 2) + Math.Pow(Y - other.Y, 2));
}

var p = new Point(1, 2);
Console.WriteLine(p.DistanceTo(new Point(4, 6)));
```
:::

:::{tab-item} TypeScript
```typescript
class Point {
  constructor(
    public readonly x: number,
    public readonly y: number,
  ) {}

  distanceTo(other: Point): number {
    return Math.hypot(this.x - other.x, this.y - other.y);
  }
}

const p = new Point(1, 2);
console.log(p.distanceTo(new Point(4, 6)));
```
:::

:::{tab-item} JavaScript
```javascript
class Point {
  constructor(x, y) {
    this.x = x;
    this.y = y;
  }

  distanceTo(other) {
    const dx = this.x - other.x;
    const dy = this.y - other.y;
    return Math.hypot(dx, dy);
  }
}

const p = new Point(1, 2);
console.log(p.distanceTo(new Point(4, 6)));
```
:::

:::{tab-item} Kotlin
```kotlin
import kotlin.math.hypot

data class Point(val x: Double, val y: Double) {
    fun distanceTo(other: Point): Double =
        hypot(x - other.x, y - other.y)
}

fun main() {
    val p = Point(1.0, 2.0)
    println(p.distanceTo(Point(4.0, 6.0)))
}
```
:::

:::{tab-item} Scala
```scala
import scala.math.hypot

final case class Point(x: Double, y: Double):
  def distanceTo(other: Point): Double =
    hypot(x - other.x, y - other.y)

@main def run(): Unit =
  val p = Point(1.0, 2.0)
  println(p.distanceTo(Point(4.0, 6.0)))
```
:::

:::{tab-item} Java
```java
public final class Point {
    private final double x;
    private final double y;

    public Point(double x, double y) {
        this.x = x;
        this.y = y;
    }

    public double distanceTo(Point other) {
        return Math.hypot(this.x - other.x, this.y - other.y);
    }

    public static void main(String[] args) {
        Point p = new Point(1, 2);
        System.out.println(p.distanceTo(new Point(4, 6)));
    }
}
```
:::

:::{tab-item} Bash
```bash
#!/usr/bin/env bash
# Bash has no classes. We approximate one with an associative array
# for state and a function family that takes the "instance" as its
# first argument.

point_new() {
    local -n self=$1
    self=([x]=$2 [y]=$3)
}

point_distance_to() {
    local -n a=$1
    local -n b=$2
    awk -v ax="${a[x]}" -v ay="${a[y]}" \
        -v bx="${b[x]}" -v by="${b[y]}" \
        'BEGIN { print sqrt((ax-bx)^2 + (ay-by)^2) }'
}

declare -A p q
point_new p 1 2
point_new q 4 6
point_distance_to p q
```
:::

::::

## Deconstruct

Pulling the fields back out of a `Point` instance:

::::{tab-set}

:::{tab-item} Zig
```zig
const p = Point.init(1.0, 2.0);
const x = p.x;
const y = p.y;
```
:::

:::{tab-item} Go
```go
p := NewPoint(1, 2)
x, y := p.X, p.Y
```
:::

:::{tab-item} Python
```python
from dataclasses import astuple

p = Point(1.0, 2.0)
x, y = astuple(p)
```
:::

:::{tab-item} Rust
```rust
let p = Point::new(1.0, 2.0);
let Point { x, y } = p;
```
:::

:::{tab-item} C
```c
Point p = point_new(1.0, 2.0);
double x = p.x;
double y = p.y;
```
:::

:::{tab-item} C++
```cpp
struct Point { double x, y; };
Point p{1.0, 2.0};
auto [x, y] = p;
```
:::

:::{tab-item} C#
```csharp
public sealed class Point
{
    public double X { get; }
    public double Y { get; }
    public Point(double x, double y) { X = x; Y = y; }
    public void Deconstruct(out double x, out double y) { x = X; y = Y; }
}

var (x, y) = new Point(1, 2);
```
:::

:::{tab-item} TypeScript
```typescript
const p = new Point(1, 2);
const { x, y } = p;
```
:::

:::{tab-item} JavaScript
```javascript
const p = new Point(1, 2);
const { x, y } = p;
```
:::

:::{tab-item} Kotlin
```kotlin
val p = Point(1.0, 2.0)
val (x, y) = p
```
:::

:::{tab-item} Scala
```scala
val p = Point(1.0, 2.0)
val Point(x, y) = p
```
:::

:::{tab-item} Java
```java
record Point(double x, double y) {}

Point p = new Point(1, 2);
if (p instanceof Point(double x, double y)) {
    System.out.println(x + ", " + y);
}
```
:::

:::{tab-item} Bash
```bash
declare -A p
point_new p 1 2
x="${p[x]}"
y="${p[y]}"
```
:::

::::
