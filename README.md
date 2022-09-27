# rust-eq

This was my term project for Fault Tolerant Computing. This is a proof of concept program that detects semantic differences between rust programs.

To run the project, just clone and `cargo run`. This will display a help message with further info.

This program reads two files, then looks at all functions inside of them. It will cross-reference all of the function with common names (eg `foo()` and `bar::baz()`) for testing. Testing is done via symbolic execution.

## Paper

In the root directory of this project is an IEEE formatted paper on the topic of "Quantifying Fault Introductions Across Software Versions". It uses this project to show how program equivalence testing could be beneficial for cross-version testing.

## Presentation

An org-reveal presentation on this project lives in the `docs` directory. You can view it in your browser at https://pohldev.in/rust-eq/.
