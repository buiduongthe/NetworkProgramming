import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.FlowLayout;
import java.awt.GridLayout;
import java.net.Socket;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import javax.swing.BorderFactory;
import javax.swing.JButton;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JTextArea;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;
import javax.swing.Timer;
import javax.swing.WindowConstants;

/**
 * Bài 5.3 – Ứng dụng tra cứu có giao diện Swing (Mục 5.3.2).
 * Nút Kết nối: Mã giả 5.3 – cách (1) luồng nền + UI_INVOKE (SwingUtilities.invokeLater).
 * Nút Tra cứu: Mã giả 5.4 – cách (2) bất đồng bộ: yêu cầu chạy trong CompletableFuture trên luồng ảo;
 * Swing không tự đưa phần tiếp theo về Event Dispatch Thread như SynchronizationContext của .NET, nên
 * phần tiếp theo (whenComplete) phải chuyển về EDT bằng invokeLater.
 * Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.8 (một lượt tra cứu), 5.1 (khung JSON).
 * Chạy: java -cp out LookupGui [host] [port]      (mặc định 127.0.0.1 9100)
 */
public class LookupGui {
    final JFrame frame = new JFrame("Tra cứu thuật ngữ – Bài 5.3");
    final JTextField txtHost = new JTextField(12), txtPort = new JTextField(6), txtTerm = new JTextField(26);
    final JButton btnConnect = new JButton("Kết nối"), btnLookup = new JButton("Tra cứu");
    final JTextArea txtResult = new JTextArea(3, 46);
    final JLabel lblStatus = new JLabel("Chưa kết nối"), lblClock = new JLabel("", JLabel.RIGHT);
    final CompletableFuture<Void> ct = Khung.newCancellation();     // ct: bị hủy khi đóng cửa sổ (Mục 5.4)
    final ExecutorService vexec = Executors.newVirtualThreadPerTaskExecutor();
    volatile Socket conn;                                            // conn: trường của cửa sổ
    int ticks;

    LookupGui(String host, int port) {
        txtHost.setText(host);
        txtPort.setText(String.valueOf(port));
        btnLookup.setEnabled(false);
        txtResult.setEditable(false);
        txtResult.setLineWrap(true);
        txtResult.setBorder(BorderFactory.createLineBorder(Color.GRAY));
        lblClock.setForeground(Color.DARK_GRAY);
        JPanel row1 = new JPanel(new FlowLayout(FlowLayout.LEFT));
        row1.add(new JLabel("Máy chủ:")); row1.add(txtHost); row1.add(new JLabel("Cổng:")); row1.add(txtPort); row1.add(btnConnect);
        JPanel row2 = new JPanel(new FlowLayout(FlowLayout.LEFT));
        row2.add(new JLabel("Thuật ngữ:")); row2.add(txtTerm); row2.add(btnLookup);
        JPanel row4 = new JPanel(new GridLayout(1, 2));
        row4.add(lblStatus); row4.add(lblClock);
        JPanel top = new JPanel(new GridLayout(2, 1));
        top.add(row1); top.add(row2);
        JPanel root = new JPanel(new BorderLayout(4, 4));
        root.setBorder(BorderFactory.createEmptyBorder(6, 8, 8, 8));
        root.add(top, BorderLayout.NORTH); root.add(txtResult, BorderLayout.CENTER); root.add(row4, BorderLayout.SOUTH);
        frame.setContentPane(root);
        btnConnect.addActionListener(e -> onConnectClick());
        btnLookup.addActionListener(e -> onLookupClick());
        new Timer(100, e -> lblClock.setText(String.format(java.util.Locale.ROOT,
                "Đồng hồ giao diện: %.1f s", ++ticks / 10.0))).start();  // đồng hồ: chứng tỏ UI không đứng hình
        frame.setDefaultCloseOperation(WindowConstants.DISPOSE_ON_CLOSE);
        frame.addWindowListener(new java.awt.event.WindowAdapter() {
            @Override public void windowClosed(java.awt.event.WindowEvent e) { close(); }
        });
        frame.setSize(600, 200);
        frame.setLocation(0, 0);
    }

    static void uiInvoke(Runnable proc) { SwingUtilities.invokeLater(proc); }   // UI_INVOKE
    void showStatus(String text) { lblStatus.setText(text); }

    // ---- Mã giả 5.3: cách (1) luồng nền và UI_INVOKE ------------------------------------------
    void onConnectClick() {                                          // trên Event Dispatch Thread
        btnConnect.setEnabled(false);
        String host = txtHost.getText();
        int port = Integer.parseInt(txtPort.getText().trim());
        new Thread(() -> connectWorker(host, port)).start();         // SPAWN_THREAD
    }

    void connectWorker(String host, int port) {                      // Mã giả 5.3, dòng 5–14: luồng nền
        try {
            Socket c = Khung.safeConnect(host, port, 3000);          // dòng 7: chặn luồng nền
            Khung.linkClose(ct, c);
            uiInvoke(() -> { conn = c; btnLookup.setEnabled(true); });   // dòng 8: SET_FIELD(conn, c)
            uiInvoke(() -> showStatus("Đã kết nối"));                // dòng 9
        } catch (Khung.NetError e) {                                 // dòng 10–12
            uiInvoke(() -> showStatus("Lỗi: " + e.code));
            uiInvoke(() -> btnConnect.setEnabled(true));             // SET_ENABLED(btnConnect, TRUE)
        }
    }

    // ---- Mã giả 5.4: cách (2) trình xử lý bất đồng bộ ------------------------------------------
    void onLookupClick() {
        btnLookup.setEnabled(false);                                 // chống bấm lặp
        lblStatus.setText("Đang tra cứu...");
        Map<String, Object> req = Json.obj("op", "lookup", "term", txtTerm.getText());
        CompletableFuture.supplyAsync(() -> {                        // phần chờ mạng chạy trên luồng ảo
            try {
                Khung.sendJson(conn, req, ct);
                return (Map<?, ?>) Khung.recvJson(conn, ct);
            } catch (java.io.IOException e) {
                throw new java.util.concurrent.CompletionException(e);
            }
        }, vexec).whenComplete((resp, err) -> uiInvoke(() -> {       // phần tiếp theo: về EDT
            if (!frame.isDisplayable()) return;                      // cửa sổ đã đóng
            if (err == null) {
                txtResult.setText(Long.valueOf(200).equals(resp.get("status"))   // dòng 8–12
                        ? String.valueOf(resp.get("text")) : "(" + resp.get("status") + ") " + resp.get("error"));
                lblStatus.setText("Xong");
            } else {
                Throwable cause = err.getCause() != null ? err.getCause() : err;
                lblStatus.setText("Lỗi: " + cause.getMessage());
            }
            btnLookup.setEnabled(true);                              // FINALLY: bật lại nút
        }));
    }

    void close() {                                                   // đóng cửa sổ: hủy tác vụ, đóng kết nối
        Khung.cancel(ct);
        vexec.shutdown();
    }

    public static void main(String[] args) {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int port = args.length > 1 ? Integer.parseInt(args[1]) : 9100;
        SwingUtilities.invokeLater(() -> new LookupGui(host, port).frame.setVisible(true));
    }
}
