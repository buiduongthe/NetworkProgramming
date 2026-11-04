/** Lỗi cấu hình (Mã giả 2.1): giá trị địa chỉ hoặc cổng không dùng được. */
class ConfigError extends Exception {
    ConfigError(String message) {
        super(message);
    }
}
