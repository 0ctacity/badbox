pub fn crowded(value: usize) usize {
    const first = @as(usize, value);
    const second = @as(usize, value);
    return first + second;
}

pub fn acceptable(value: usize) usize {
    return @as(usize, value);
}
