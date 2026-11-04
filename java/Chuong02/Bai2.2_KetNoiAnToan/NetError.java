/** Lỗi mạng đã phân loại (Mã giả 2.2): kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}; retry: có nên thử lại. */
class NetError extends Exception {
    final String kind;
    final boolean retry;

    NetError(String kind, boolean retry, Throwable cause) {
        super(kind, cause);
        this.kind = kind;
        this.retry = retry;
    }
}
