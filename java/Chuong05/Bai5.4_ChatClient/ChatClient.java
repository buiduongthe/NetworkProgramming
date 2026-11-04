import java.io.IOException;
import java.net.Socket;
import java.util.Map;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.locks.ReentrantLock;
import java.util.function.Consumer;

/**
 * Bài 5.4 – Lớp dịch vụ ChatClient (Mã giả 5.5, Mục 5.3.3).
 * Kế thừa: Mã giả 5.1 (khung JSON); SAFE_CONNECT_ASYNC = Mã giả 2.2 chạy trên luồng ảo.
 * Ánh xạ Java: RUN_BACKGROUND ↔ Thread.ofVirtual().start; cts ↔ CompletableFuture&lt;Void&gt;
 * (hủy thì đóng socket); send_lock ↔ ReentrantLock (khóa chặn, dùng với luồng ảo).
 * Lớp không tham chiếu tới điều khiển giao diện nào; tầng giao diện chỉ đăng ký onMessage, onStatus.
 */
class ChatClient {
    Socket sock;
    CompletableFuture<Void> cts;                                     // tín hiệu hủy dùng chung
    final ReentrantLock sendLock = new ReentrantLock();              // send_lock: mỗi lúc gửi một khung
    Consumer<Map<?, ?>> onMessage = m -> { };                        // sự kiện cho UI
    Consumer<String> onStatus = s -> { };

    void connect(String host, int port, String name) throws IOException {   // dòng 3–14
        sock = Khung.safeConnect(host, port, 3000);
        cts = Khung.newCancellation();                               // NEW_CANCELLATION()
        Khung.linkClose(cts, sock);
        Map<String, Object> hello = Json.obj("type", "hello", "name", name);
        try {
            Khung.sendJson(sock, hello, cts);
        } catch (IOException e) {
            Khung.closeQuietly(sock);                                // không để rò socket
            throw e;
        }
        Thread.ofVirtual().start(this::receiveLoop);                 // RUN_BACKGROUND: không chờ
        onStatus.accept("connected");
    }

    void receiveLoop() {                                             // dòng 15–27
        try {
            while (true) {
                Map<?, ?> msg = (Map<?, ?>) Khung.recvJson(sock, cts);
                onMessage.accept(msg);                               // RAISE_EVENT(on_message, msg)
            }
        } catch (CancellationException e) {
            // ngắt kết nối chủ động: kết thúc lặng lẽ
        } catch (IOException | IllegalArgumentException | ClassCastException e) {   // dòng 23; JSON sai là
            Khung.closeQuietly(sock);                                // IllegalArgumentException; giải phóng socket
            onStatus.accept("lost: " + e.getMessage());
        }
    }

    void sendMessage(String room, String text) throws IOException {  // dòng 28–36
        sendLock.lock();                                             // ACQUIRE(send_lock)
        try {
            Khung.sendJson(sock, Json.obj("type", "msg", "room", room, "text", text), cts);
        } finally {
            sendLock.unlock();                                       // RELEASE(send_lock)
        }
    }

    void disconnect() {                                              // dòng 37–39
        if (cts != null) Khung.cancel(cts);                          // hủy trước (linkClose đóng socket)
        if (sock != null) Khung.closeQuietly(sock);                  // đóng socket sau
    }
}
