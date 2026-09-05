use std::net::TcpListener;

/// 固定区间(Tauri capability remote urls 按此枚举授权,见 capabilities/default.json)
pub const PORT_RANGE: std::ops::RangeInclusive<u16> = 8977..=8999;

/// 区间内逐个试绑,第一个能 bind 的即用(本机防冲突,效果等价随机端口)
pub fn pick_free_port() -> Option<u16> {
    PORT_RANGE.clone().find(|&p| TcpListener::bind(("127.0.0.1", p)).is_ok())
}

#[cfg(test)]
mod tests {
    #[test]
    fn picked_port_is_in_range_and_bindable() {
        let p = super::pick_free_port().expect("8977..=8999 全被占用的可能性极低");
        assert!(super::PORT_RANGE.contains(&p));
        assert!(std::net::TcpListener::bind(("127.0.0.1", p)).is_ok());
    }
}
