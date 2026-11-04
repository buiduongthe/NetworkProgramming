import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.StandardSocketOptions;
import java.nio.ByteBuffer;
import java.nio.channels.SelectionKey;
import java.nio.channels.Selector;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.HashSet;

/**
 * Bài 6.3 – Server echo một luồng dùng dồn kênh vào/ra, mô hình reactor (Mã giả 6.4, Mục 6.2.4).
 * Kế thừa Mã giả 3.1: cùng chức năng echo; vòng ACCEPT/RECV chặn được thay bằng vòng Selector.select()
 * (java.nio) trên tập kênh không chặn.
 * Chạy: java -cp out ReactorServer [host] [port]      (mặc định 0.0.0.0 9000)
 */
public class ReactorServer {
    static final int MAX_BUF = 65536;   // Mã giả 6.4 dòng 4: ngưỡng bộ đệm gửi của mỗi kết nối (64 KiB)

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        ServerSocketChannel srv = ServerSocketChannel.open();          // như Mã giả 3.1
        srv.setOption(StandardSocketOptions.SO_REUSEADDR, true);
        srv.bind(new InetSocketAddress(host, port), 16);
        srv.configureBlocking(false);                                   // SETOPT(srv, NONBLOCKING, TRUE)
        Selector sel = Selector.open();
        srv.register(sel, SelectionKey.OP_ACCEPT);
        Map<SocketChannel, byte[]> outBuf = new LinkedHashMap<>();      // dòng 4: kênh → dữ liệu chờ gửi
        Set<SocketChannel> closing = new HashSet<>();                   // dòng 5: client đã đóng chiều gửi
        Map<SocketChannel, String> names = new HashMap<>();             // chỉ để ghi nhật ký
        Map<SocketChannel, Long> echoed = new HashMap<>();
        ByteBuffer chunk = ByteBuffer.allocate(4096);
        System.out.println("[server] Reactor một luồng đang lắng nghe tại " + host + ":" + port + ", MAX_BUF = " + MAX_BUF);
        while (true) {
            for (Map.Entry<SocketChannel, byte[]> e : outBuf.entrySet()) {
                int ops = 0;
                if (!closing.contains(e.getKey()) && e.getValue().length < MAX_BUF)
                    ops |= SelectionKey.OP_READ;                                    // dòng 8–9: room
                if (e.getValue().length > 0) ops |= SelectionKey.OP_WRITE;         // dòng 7: pending
                e.getKey().keyFor(sel).interestOps(ops);
            }
            sel.select();                                               // dòng 10: SELECT({srv} ∪ room, pending)
            List<SelectionKey> ready = new ArrayList<>(sel.selectedKeys());
            sel.selectedKeys().clear();
            for (SelectionKey k : ready) {                              // FOR EACH s IN readable
                if (k.isAcceptable()) {
                    SocketChannel conn = srv.accept();                  // có kết nối mới
                    if (conn == null) continue;
                    conn.configureBlocking(false);                      // SETOPT(conn, NONBLOCKING, TRUE)
                    conn.register(sel, SelectionKey.OP_READ);
                    outBuf.put(conn, new byte[0]);                      // EMPTY_BYTES
                    InetSocketAddress a = (InetSocketAddress) conn.getRemoteAddress();
                    names.put(conn, a.getAddress().getHostAddress() + ":" + a.getPort());
                    echoed.put(conn, 0L);
                    System.out.println("[server] Client " + names.get(conn) + " kết nối (đang mở: " + outBuf.size() + ")");
                } else if (k.isValid() && k.isReadable()) {
                    SocketChannel s = (SocketChannel) k.channel();
                    chunk.clear();
                    int n;
                    try {
                        n = s.read(chunk);                              // dòng 17: kênh đã sẵn sàng, không chờ
                    } catch (IOException e) {                           // ví dụ Connection reset: đóng ngay
                        close(s, outBuf, closing, names, echoed);
                        continue;
                    }
                    if (n == -1) {                                      // dòng 18–19: client gửi FIN
                        closing.add(s);                                 // gửi nốt rồi mới đóng
                        System.out.println("[server] " + names.get(s) + " gửi FIN, còn " + outBuf.get(s).length + " byte chờ gửi");
                    } else if (n > 0) outBuf.put(s, concat(outBuf.get(s), chunk.array(), n));   // dòng 21: chờ gửi
                }
            }
            for (SelectionKey k : ready) {                              // dòng 25–28: FOR EACH s IN writable
                if (!(k.channel() instanceof SocketChannel s) || !outBuf.containsKey(s)) continue;
                if (!k.isValid() || !k.isWritable()) continue;
                byte[] buf = outBuf.get(s);
                int n;
                try {
                    n = s.write(ByteBuffer.wrap(buf));                  // có thể gửi một phần
                } catch (IOException e) {
                    close(s, outBuf, closing, names, echoed);
                    continue;
                }
                outBuf.put(s, java.util.Arrays.copyOfRange(buf, n, buf.length));   // out_buf[s][n .. END]
                echoed.put(s, echoed.get(s) + n);
            }
            for (SocketChannel s : new ArrayList<>(closing))            // dòng 29–33: COPY(closing)
                if (outBuf.get(s).length == 0) close(s, outBuf, closing, names, echoed);   // đã gửi hết: đóng
        }
    }

    static byte[] concat(byte[] a, byte[] b, int n) {
        byte[] r = java.util.Arrays.copyOf(a, a.length + n);
        System.arraycopy(b, 0, r, a.length, n);
        return r;
    }

    static void close(SocketChannel s, Map<SocketChannel, byte[]> outBuf, Set<SocketChannel> closing,
                      Map<SocketChannel, String> names, Map<SocketChannel, Long> echoed) {
        outBuf.remove(s);                                               // REMOVE(out_buf, s)
        closing.remove(s);                                              // REMOVE(closing, s)
        try { s.close(); } catch (IOException ignored) { }              // CLOSE(s), tự hủy đăng ký selector
        System.out.println("[server] Đóng " + names.remove(s) + ": đã echo " + echoed.remove(s)
                + " byte, còn mở " + outBuf.size());
    }
}
