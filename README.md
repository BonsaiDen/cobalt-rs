# cobalt - low level, reliable UDP based networking

A implementation of a basic, reliable UDP based networking protocol in [rust](https://rust-lang.org/).

Mostly designed after the [UDP vs. TCP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/) 
article series by Glenn Fiedler.

- [Documentation](https://bonsaiden.github.io/cobalt-rs/)


## TODO

- Move out logic for congestion detection / avoidance into a trait so there can be different implementation.


## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
cobalt = "0.1.0"
```

and this to your crate root:

```rust
extern crate cobalt;
```


## Licensed under MIT

Copyright (c) 2015 Ivo Wetzel.

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

