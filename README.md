# cobalt [![Build Status](https://img.shields.io/travis/BonsaiDen/cobalt-rs.svg?style=flat-square)](https://travis-ci.org/BonsaiDen/cobalt-rs) [![Build status](https://img.shields.io/appveyor/ci/BonsaiDen/cobalt-rs.svg?style=flat-square)](https://ci.appveyor.com/project/BonsaiDen/cobalt-rs) [![Crates.io](https://img.shields.io/crates/v/cobalt.svg?style=flat-square)](https://crates.io/crates/cobalt) 

A [rust](https://rust-lang.org/) based networking library which provides [virtual 
connections over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/) .
It also comes with a messaging layer for the sending of un-/reliable and/or 
ordered messages.

- [Documentation](https://bonsaiden.github.io/cobalt-rs/doc/cobalt)


## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
cobalt = "0.5.0"
```

and this to your crate root:

```rust
extern crate cobalt;
```

You can also enable optional features such as handlers for lost packets or
packet compression inside `Cargo.toml`:

```toml
[dependencies.cobalt]
version = "0.5.0"
features = ["packet_handler_lost", "packet_handler_compress"]
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

