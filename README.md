# cobalt [![Build Status](https://travis-ci.org/BonsaiDen/cobalt-rs.svg)](https://travis-ci.org/BonsaiDen/cobalt-rs)[![Crates.io](https://img.shields.io/crates/v/cobalt.svg?style=flat-square)](https://crates.io/crates/cobalt)

A [rust](https://rust-lang.org/) based networking library which provides [virtual 
connections over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/) 
and provides a messaging layer for sending reliable and/or ordered messages.

- [Documentation](https://bonsaiden.github.io/cobalt-rs/doc/cobalt)


## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
cobalt = "0.3.0"
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

