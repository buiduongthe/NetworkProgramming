import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.Socket;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.Map;
import java.util.Random;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;
import java.util.concurrent.Executors;
import java.util.function.BiConsumer;

/**
 * Bài 5.5 – Tải tệp có báo cáo tiến trình và hủy (Mã giả 5.6 PUT_FILE_ASYNC, Mục 5.3.4).
 * Kế thừa Mã giả 3.10 (PUT_CLIENT): giữ nguyên giao thức và trình tự 4 bước; thêm onProgress
 * (tối đa ~10 lần/giây) và tín hiệu hủy ct. Thủ tục chạy trên luồng ảo (CompletableFuture); hủy = đóng
 * socket (Khung.linkClose) nên lời gọi đang chặn kết thúc ngay. Nếu tệp chưa có, tạo tệp thử 4 MiB.
 * Chạy: java -cp out PutClient [host] [port] [tệp] [hủy_sau_ms]   (mặc định 127.0.0.1 9100 du_lieu_thu.bin)
 */
public class PutClient {
    static Map<?, ?> putFile(Socket s, Path path, BiConsumer<Long, Long> onProgress,
                             CompletableFuture<Void> ct) throws Exception {
        long size = Files.size(path);
        MessageDigest md = MessageDigest.getInstance("SHA-256");    // tệp lớn: lâu
        try (InputStream in = Files.newInputStream(path)) {
            byte[] b = new byte[65536];
            for (int n; (n = in.read(b)) > 0; ) md.update(b, 0, n);
        }
        String digest = HexFormat.of().formatHex(md.digest());
        Khung.sendJson(s, Json.obj("op", "put", "name", path.getFileName().toString(), "size", size, "sha256", digest), ct);
        if (!Long.valueOf(100).equals(((Map<?, ?>) Khung.recvJson(s, ct)).get("status")))
            throw new Khung.ProtocolError("Server từ chối");
        long sent = 0, last = 0;
        byte[] chunk = new byte[65536];
        try (FileInputStream f = new FileInputStream(path.toFile())) {   // try-with-resources = FINALLY CLOSE_FILE
            for (int n; (n = f.read(chunk)) > 0; ) {                 // READ(f, 65536)
                if (ct.isDone()) throw new CancellationException("Đã hủy");
                try {
                    s.getOutputStream().write(chunk, 0, n);          // SEND_ALL: điểm có thể hủy
                } catch (IOException e) {
                    if (ct.isDone()) throw new CancellationException("Đã hủy");
                    throw e;
                }
                sent += n;                                           // đã vào bộ đệm gửi, chưa chắc tới server
                if (System.currentTimeMillis() - last >= 100) {      // tối đa ~10 lần/giây
                    onProgress.accept(sent, size);
                    last = System.currentTimeMillis();
                }
            }
        }
        Map<?, ?> resp = (Map<?, ?>) Khung.recvJson(s, ct);         // dòng 24: server đã nhận đủ
        if (Long.valueOf(200).equals(resp.get("status"))) onProgress.accept(size, size);   // dòng 25
        return resp;
    }

    static void showProgress(long sent, long size) {                // onProgress
        System.out.printf(java.util.Locale.ROOT, "[client] Tiến trình: %.2f/%.2f MiB (%d%%)%n",
                sent / 1048576.0, size / 1048576.0, sent * 100 / size);
    }

    public static void main(String[] args) throws Exception {
        Object[] ep = Khung.parseEndpoint(java.util.Arrays.copyOf(args, Math.min(args.length, 2)), "127.0.0.1", 9100);
        Path path = Path.of(args.length > 2 ? args[2] : "du_lieu_thu.bin");
        int cancelMs = args.length > 3 ? Integer.parseInt(args[3]) : 0;
        if (!Files.exists(path)) {
            byte[] data = new byte[4 * 1048576];
            new Random(5).nextBytes(data);
            Files.write(path, data);
            System.out.println("[client] Tạo tệp thử " + path + " (4 MiB)");
        }
        Socket s;
        try { s = Khung.safeConnect((String) ep[0], (Integer) ep[1], 3000); }
        catch (Khung.NetError e) { System.out.println("[client] Không kết nối được: " + e.getMessage()); return; }
        s.setSendBufferSize(65536);         // bộ đệm gửi nhỏ để số byte "đã gửi" bám sát số byte server đã nhận
        System.out.println("[client] Đã kết nối " + ep[0] + ":" + ep[1] + ", tải lên " + path
                + (cancelMs > 0 ? ", sẽ bấm Hủy sau " + cancelMs + " ms" : ""));
        CompletableFuture<Void> cts = cancelMs > 0 ? Khung.cancelAfter(cancelMs) : Khung.newCancellation();  // nút Hủy
        Khung.linkClose(cts, s);
        long t0 = System.nanoTime();
        try (s; var vexec = Executors.newVirtualThreadPerTaskExecutor()) {
            Map<?, ?> resp = CompletableFuture.supplyAsync(() -> {
                try { return putFile(s, path, PutClient::showProgress, cts); }
                catch (CancellationException e) { throw e; }
                catch (Exception e) { throw new CompletionException(e); }
            }, vexec).join();                                        // AWAIT PUT_FILE_ASYNC(...)
            System.out.printf(java.util.Locale.ROOT, "[client] Kết quả: %s sau %.2f s%n", Json.stringify(resp), (System.nanoTime() - t0) / 1e9);
        } catch (CancellationException | CompletionException e) {
            if (e instanceof CancellationException || e.getCause() instanceof CancellationException)
                System.out.printf(java.util.Locale.ROOT, "[client] Đã hủy sau %.2f s; đóng kết nối (trạng thái giao thức không còn tin cậy)%n",
                        (System.nanoTime() - t0) / 1e9);
            else
                System.out.println("[client] Lỗi: " + e.getCause().getMessage());
        }
    }
}
