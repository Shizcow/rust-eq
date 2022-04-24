pub fn foo(x: u32) -> u32 {
    x%4
}









pub mod nested {
    pub fn bar(x: u32) -> u32 {
	if x == 1 {
	    return 0;
	}
	panic!("Other");
    }
}
