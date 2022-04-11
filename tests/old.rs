pub fn foo(x: u32) -> u32 {
    if x%4 == 0 {
	return 0;
    } else if x%4 == 1 {
	return 1;
    } else if x%4 == 2 {
	return 2;
    } else { // if x%4 == 3
	return 3;
    }
}
