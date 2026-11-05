import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.Font;
import java.io.IOException;
import java.util.LinkedHashMap;
import java.util.Map;
import javax.swing.BorderFactory;
import javax.swing.BoxLayout;
import javax.swing.ButtonGroup;
import javax.swing.JButton;
import javax.swing.JCheckBox;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JRadioButton;
import javax.swing.JSpinner;
import javax.swing.JTextField;
import javax.swing.SpinnerNumberModel;
import javax.swing.SwingUtilities;
import javax.swing.Timer;

/**
 * Bài mở rộng MR6.5 – Người xem phát trực tiếp có giao diện Swing.
 *
 * Lõi NguoiXem nhận, giải mã, phát âm thanh (đồng hồ chủ) và chọn khung để hiện; cửa sổ cứ 50 ms đọc khung đang hiện
 * để vẽ (phóng ×2) và cứ 200 ms cập nhật thanh mức, thống kê. Điều khiển: Kết nối/Ngắt, công tắc Đồng bộ tiếng-hình,
 * chọn nhận Âm thanh / Video / Cả hai, độ sâu bộ đệm jitter, Phát ra loa (có thiết bị phát), Chụp (lưu
 * out/<tên>_gui_chup_N.png).
 *
 * Chạy:  java -cp out XemGui [tên] [host] [cổng_tcp]
 */
public class XemGui {
    final JFrame frame;
    final String tenBanDau;
    final JTextField txtIp, txtCong, txtTen;
    final JButton btnKetNoi = GiaoDienChung.nut("Kết nối", GiaoDienChung.XANH_LA, 130);
    final GiaoDienChung.KhungAnh khungAnh = new GiaoDienChung.KhungAnh(2 * Chung.RONG, 2 * Chung.CAO);
    final JLabel lblKhung = GiaoDienChung.nhan("Khung: –");
    final GiaoDienChung.ThanhMuc thanh = new GiaoDienChung.ThanhMuc(200);
    final JCheckBox chkLoa = new JCheckBox("Loa");
    final JLabel lblLoa;
    final JCheckBox chkDongBo = new JCheckBox("Đồng bộ tiếng-hình (tiếng làm đồng hồ chủ)", true);
    final JRadioButton rdAm = new JRadioButton("Âm thanh"), rdVideo = new JRadioButton("Video"), rdCaHai = new JRadioButton("Cả hai", true);
    final JSpinner spnDem = new JSpinner(new SpinnerNumberModel(Chung.DO_SAU_MAC_DINH, 1, 25, 1));
    final JButton btnChup = GiaoDienChung.nut("Chụp", GiaoDienChung.XANH_DUONG, 80);
    final Map<String, JLabel> nhanTk = new LinkedHashMap<>();
    final GiaoDienChung.NhatKy nhatKy;
    final GiaoDienChung.ThongKeDinhKy dinhKy;
    final JLabel thanhTt = GiaoDienChung.thanhTrangThai("Trạng thái: chưa kết nối");
    final ThietBiAmThanh.LoaThat loa;
    String nhanTt = "chưa kết nối";
    volatile NguoiXem lop;
    long soDaVe = -1;
    int soChup;
    final Timer henHinh = new Timer(50, e -> veHinh()), henTk = new Timer(200, e -> capNhat());

    XemGui(String ten, String host, int cong) {
        tenBanDau = ten;
        frame = new JFrame("Người xem trực tiếp – MR6.5 (" + ten + ")");
        nhatKy = new GiaoDienChung.NhatKy(5, 70, "xem " + ten);
        dinhKy = new GiaoDienChung.ThongKeDinhKy("xem " + ten);
        ThietBiAmThanh.KetQuaLoa kq = ThietBiAmThanh.moLoa();
        loa = kq.loa();
        txtIp = GiaoDienChung.oNhap(host, 11);
        txtCong = GiaoDienChung.oNhap(cong, 5);
        txtTen = GiaoDienChung.oNhap(ten, 7);

        JPanel g = GiaoDienChung.khungNhom("Kết nối server");
        g.setLayout(new BorderLayout());
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("IP:"), txtIp, GiaoDienChung.nhan(" Port:"), txtCong,
                GiaoDienChung.nhan(" Tên:"), txtTen, javax.swing.Box.createHorizontalStrut(6), btnKetNoi), BorderLayout.WEST);
        JPanel tren = g;

        JPanel giua = new JPanel();
        giua.setLayout(new BoxLayout(giua, BoxLayout.X_AXIS));
        giua.setBackground(GiaoDienChung.NEN);
        g = GiaoDienChung.khungNhom("Hình (phóng ×2)");
        g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
        khungAnh.setAlignmentX(0);
        khungAnh.datChu("Chưa kết nối");
        g.add(khungAnh);
        lblKhung.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 11));
        g.add(lblKhung);
        chkLoa.setBackground(GiaoDienChung.NEN);
        chkLoa.setSelected(loa != null);
        chkLoa.setEnabled(loa != null);
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Tiếng:"), thanh, chkLoa));
        lblLoa = GiaoDienChung.nhan(loa != null ? "Loa: THẬT – thiết bị mặc định" : "Loa: " + kq.lyDo());
        lblLoa.setForeground(loa != null ? new Color(0x006000) : new Color(0x8a4b00));
        g.add(lblLoa);
        g.setMaximumSize(new Dimension(g.getPreferredSize().width, Integer.MAX_VALUE));
        giua.add(g);

        JPanel phai = new JPanel();
        phai.setLayout(new BoxLayout(phai, BoxLayout.Y_AXIS));
        phai.setBackground(GiaoDienChung.NEN);
        g = GiaoDienChung.khungNhom("Điều khiển");
        g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
        g.setAlignmentX(0);
        chkDongBo.setBackground(GiaoDienChung.NEN);
        g.add(GiaoDienChung.hang(chkDongBo));
        ButtonGroup nhom = new ButtonGroup();
        for (JRadioButton r : new JRadioButton[] {rdAm, rdVideo, rdCaHai}) {
            r.setBackground(GiaoDienChung.NEN);
            nhom.add(r);
            r.addActionListener(e -> apDangKy());
        }
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Nhận:"), rdAm, rdVideo, rdCaHai));
        g.add(GiaoDienChung.hang(GiaoDienChung.nhan("Bộ đệm jitter (khung 20 ms):"), spnDem, javax.swing.Box.createHorizontalStrut(8), btnChup));
        phai.add(g);
        g = GiaoDienChung.khungNhom("Thống kê");
        g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
        g.setAlignmentX(0);
        String[][] muc = {{"tre", "Trễ đầu-cuối (tiếng)*"}, {"lech", "Lệch tiếng-hình hiện tại"}, {"lech_tb", "Lệch TB giai đoạn này"},
                {"mat", "Tiếng nhận / mất"}, {"khung", "Khung nhận I / P"}, {"dung", "Đứng hình / PLI gửi"}, {"dau", "Khung đầu sau"}};
        for (String[] m : muc) {
            JLabel t = GiaoDienChung.nhan(m[1] + ":");
            t.setPreferredSize(new Dimension(170, t.getPreferredSize().height));
            JLabel v = GiaoDienChung.nhanDam("–");
            nhanTk.put(m[0], v);
            g.add(GiaoDienChung.hang(t, v));
        }
        JLabel chu = GiaoDienChung.nhan("* giả định cùng đồng hồ (cùng máy / NTP); lệch +: hình chậm hơn tiếng");
        chu.setFont(chu.getFont().deriveFont(10f));
        chu.setForeground(new Color(0x404040));
        g.add(chu);
        phai.add(g);
        giua.add(phai);

        g = GiaoDienChung.khungNhom("Nhật ký");
        g.setLayout(new BorderLayout());
        g.add(nhatKy, BorderLayout.CENTER);
        JPanel duoi = new JPanel(new BorderLayout(0, 4));
        duoi.setBackground(GiaoDienChung.NEN);
        duoi.add(g, BorderLayout.CENTER);
        duoi.add(thanhTt, BorderLayout.SOUTH);

        JPanel goc = new JPanel(new BorderLayout(0, 4));
        goc.setBackground(GiaoDienChung.NEN);
        goc.setBorder(BorderFactory.createEmptyBorder(4, 6, 6, 6));
        goc.add(tren, BorderLayout.NORTH);
        goc.add(giua, BorderLayout.CENTER);
        goc.add(duoi, BorderLayout.SOUTH);
        frame.setContentPane(goc);
        frame.setDefaultCloseOperation(JFrame.DO_NOTHING_ON_CLOSE);
        frame.addWindowListener(new java.awt.event.WindowAdapter() {
            @Override public void windowClosing(java.awt.event.WindowEvent e) { dong(); System.exit(0); }
        });
        frame.setResizable(false);
        frame.pack();

        btnKetNoi.addActionListener(e -> bamKetNoi());
        chkDongBo.addActionListener(e -> { if (lop != null) lop.datDongBo(chkDongBo.isSelected()); });
        spnDem.addChangeListener(e -> apDoSau());
        chkLoa.addActionListener(e -> { if (lop != null) lop.loa = chkLoa.isSelected() ? boc(loa) : null; });
        btnChup.addActionListener(e -> chup());
        henHinh.start();
        henTk.start();
    }

    /** Bọc loa thật theo giao diện Loa của lõi; lõi không đóng loa này (cửa sổ đóng khi thoát). */
    static NguoiXem.Loa boc(ThietBiAmThanh.LoaThat l) {
        if (l == null) return null;
        return new NguoiXem.Loa() {
            public void phat(short[] mau) { l.phat(mau); }
            public void dong() { }
        };
    }

    private void ghi(String text) { SwingUtilities.invokeLater(() -> nhatKy.ghi(text)); }

    // ------------------------------------------------------------ kết nối
    String nhanDangKy() { return rdAm.isSelected() ? "am_thanh" : rdVideo.isSelected() ? "video" : "ca_hai"; }

    void bamKetNoi() {
        if (lop != null && lop.daKetNoi) {
            lop.ngat();
            daNgat("đã ngắt kết nối");
            return;
        }
        String ten = txtTen.getText().strip(), host = txtIp.getText().strip();
        int cong;
        try {
            cong = Integer.parseInt(txtCong.getText().strip());
        } catch (NumberFormatException e) {
            nhanTt = "cổng không hợp lệ";
            return;
        }
        if (lop != null) {
            lop.loa = null;
            lop.dong();
        }
        String n = nhanDangKy();
        NguoiXem l = new NguoiXem(ten, this::ghi, (Integer) spnDem.getValue(), chkDongBo.isSelected(),
                !n.equals("video"), !n.equals("am_thanh"), 0.0, chkLoa.isSelected() ? boc(loa) : null);
        lop = l;
        btnKetNoi.setEnabled(false);
        btnKetNoi.setText("Đang kết nối…");
        soDaVe = -1;
        Chung.chayNen("ket-noi", () -> {
            try {
                int ma = l.ketNoi(host, cong);
                SwingUtilities.invokeLater(() -> daKetNoi(ma));
            } catch (IOException e) {
                SwingUtilities.invokeLater(() -> daNgat("không kết nối được – " + e.getMessage()));
            }
        });
    }

    void daKetNoi(int ma) {
        btnKetNoi.setEnabled(true);
        btnKetNoi.setText("Ngắt kết nối");
        btnKetNoi.setBackground(GiaoDienChung.DO);
        nhanTt = "đã kết nối, mã " + ma;
        khungAnh.datChu("Chờ khung hình…");
        for (JTextField o : new JTextField[] {txtIp, txtCong, txtTen}) o.setEnabled(false);
    }

    void daNgat(String text) {
        btnKetNoi.setEnabled(true);
        btnKetNoi.setText("Kết nối");
        btnKetNoi.setBackground(GiaoDienChung.XANH_LA);
        nhanTt = text;
        for (JTextField o : new JTextField[] {txtIp, txtCong, txtTen}) o.setEnabled(true);
    }

    // ------------------------------------------------------------ điều khiển
    void apDangKy() {
        if (lop != null) {
            String n = nhanDangKy();
            lop.dangKy(!n.equals("video"), !n.equals("am_thanh"));
        }
    }

    void apDoSau() {
        if (lop != null) {
            int n = (Integer) spnDem.getValue();
            lop.datDoSau(n);
            nhatKy.ghi("Bộ đệm jitter: " + n + " khung = " + n * 20 + " ms");
        }
    }

    /** Lưu ảnh đang hiện (có chú thích) vào out/<tên>_gui_chup_N.png; trả về đường dẫn hoặc null. */
    String chup() {
        NguoiXem.Chup c = lop != null ? lop.chup() : null;
        if (c == null) {
            nhatKy.ghi("Chưa có hình để chụp");
            return null;
        }
        NguoiXem.AnhRgb a = NguoiXem.anhChup(c);
        soChup++;
        String duong = "out/" + lop.ten + "_gui_chup_" + soChup + ".png";
        try {
            Chung.ghiPng(duong, a.rgb(), a.w(), a.h());
        } catch (IOException e) {
            nhatKy.ghi("Không ghi được " + duong + ": " + e.getMessage());
            return null;
        }
        nhatKy.ghi("Đã chụp " + duong);
        return duong;
    }

    // ------------------------------------------------------------ vẽ lại
    void veHinh() {
        NguoiXem l = lop;
        if (l == null) return;
        NguoiXem.Hien hien;
        long so;
        synchronized (l.khoa) {
            hien = l.hien;
            so = l.soHien;
        }
        if (hien != null && so != soDaVe) {
            khungAnh.datAnh(GiaoDienChung.anh(hien.rgb(), hien.w(), hien.h()));
            lblKhung.setText(Chung.f("Khung #%d  hình t=%.3f s  tiếng t=", hien.so(), hien.ts() / 1000.0)
                    + (hien.am() == null ? "–" : Chung.f("%.3f s", hien.am() / 1000)));
            soDaVe = so;
        } else if (hien == null && soDaVe != -2) {
            khungAnh.datAnh(null);
            soDaVe = -2;
        }
    }

    void capNhat() {
        NguoiXem l = lop;
        if (l == null) return;
        NguoiXem.TrangThai tt = l.trangThai();
        thanh.dat(tt.mucAm);
        if (tt.daKetNoi)
            dinhKy.inNeuDenHan(() -> Chung.f("tiếng %d gói, mất %d, mức %s dBFS, loa %s | hình I %d P %d, đứng %d, lệch %s,"
                    + " trễ %s | đồng bộ %s", tt.goiAm, tt.matAm, GiaoDienChung.db(tt.mucAm),
                    loa != null && chkLoa.isSelected() ? "BẬT" : "tắt", tt.khungI, tt.khungP, tt.dungHinh,
                    GiaoDienChung.chuMs(tt.lechHien, true), GiaoDienChung.chuMs(tt.treHien, false), tt.dongBo ? "BẬT" : "TẮT"));
        NguoiXem.GiaiDoan g = tt.giaiDoan.get(tt.giaiDoan.size() - 1);
        nhanTk.get("tre").setText(GiaoDienChung.chuMs(tt.treHien, false));
        JLabel lech = nhanTk.get("lech");
        lech.setText(GiaoDienChung.chuMs(tt.lechHien, true));
        lech.setForeground(tt.lechHien != null && Math.abs(tt.lechHien) <= 40 ? new Color(0x006000) : new Color(0xb00000));
        nhanTk.get("lech_tb").setText((g.n > 0 ? GiaoDienChung.chuMs(g.tong / g.n, true) : "–")
                + " (đồng bộ " + (g.dongBo ? "BẬT" : "TẮT") + ")");
        nhanTk.get("mat").setText(tt.goiAm + " / " + tt.matAm + " gói");
        nhanTk.get("khung").setText(tt.khungI + " / " + tt.khungP + " (hỏng " + tt.hong + ")");
        nhanTk.get("dung").setText(tt.dungHinh + " / " + tt.pliGui);
        nhanTk.get("dau").setText(tt.khungDauMs == null ? "–" : Chung.f("%.0f ms (%s)", tt.khungDauMs, tt.nguonDau));
        if (nhanTt.startsWith("đã kết nối") && !tt.daKetNoi) daNgat("mất kết nối server");
        thanhTt.setText("Trạng thái: " + (tt.daKetNoi ? Chung.f("đang xem – nhận %.0f kbit/s TB", tt.kbps) : nhanTt));
    }

    void dong() {
        henHinh.stop();
        henTk.stop();
        if (lop != null) {
            lop.loa = null;
            lop.dong();
        }
        if (loa != null) loa.dong();
        frame.dispose();
    }

    public static void main(String[] args) {
        String ten = args.length > 0 ? args[0] : "A";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43652;
        GiaoDienChung.NhatKy.inRa = true;                           // chạy riêng: nhật ký cũng ra dòng lệnh
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new XemGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
