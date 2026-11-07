package vn.edu.ltm.goithoai

import android.content.Context
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import org.webrtc.AudioSource
import org.webrtc.AudioTrack
import org.webrtc.DataChannel
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RtpReceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.audio.JavaAudioDeviceModule

/**
 * Bài mở rộng MR6.15 – đường âm thanh WebRTC của một cuộc gọi.
 *
 * Điện thoại KHÔNG nối thẳng với điện thoại bên kia: nó chỉ có MỘT RTCPeerConnection tới máy chủ trung gian (kiểu
 * SFU). Sau khi máy chủ báo "da_tra_loi", app tạo offer (âm thanh Opus gửi + nhận), chờ thu thập ICE xong (mạng LAN
 * nên chỉ có ứng viên host, vài trăm ms) rồi gửi {"loai":"sdp","sdp":{"type":"offer",...}}; máy chủ trả answer.
 * Âm thanh nhận được tự phát ra loa qua JavaAudioDeviceModule (có khử vọng AEC và khử ồn NS phần cứng nếu máy hỗ trợ).
 * Mỗi 2 giây đọc getStats(): RTT, tỉ lệ mất gói, tốc độ bit -> hiện trên màn hình và gửi "thong_ke" cho máy chủ.
 */
class AmThanhWebRtc(
    private val ctx: Context,
    private val guiTin: (JSONObject) -> Unit,
    private val khiTrangThai: (String) -> Unit,
    private val khiThongKe: (rttMs: Double?, matGoiPhanTram: Double?, kbps: Double?) -> Unit,
) {
    private val chinh = Handler(Looper.getMainLooper())
    private var pc: PeerConnection? = null
    private var nguon: AudioSource? = null
    private var mic: AudioTrack? = null
    private var maCuoc = ""
    private var daGuiOffer = false
    private var tatMic = false
    private var giuMay = false
    private var byteTruoc = -1.0
    private var tTruoc = 0L
    private val am = ctx.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var cheDoCu = AudioManager.MODE_NORMAL

    companion object {
        private var nhaMayChung: PeerConnectionFactory? = null

        /** PeerConnectionFactory dùng chung cả ứng dụng (khởi tạo thư viện native một lần). */
        @Synchronized
        fun nhaMay(ctx: Context): PeerConnectionFactory {
            nhaMayChung?.let { return it }
            val app = ctx.applicationContext
            PeerConnectionFactory.initialize(PeerConnectionFactory.InitializationOptions.builder(app).createInitializationOptions())
            val adm = JavaAudioDeviceModule.builder(app)
                .setUseHardwareAcousticEchoCanceler(true)
                .setUseHardwareNoiseSuppressor(true)
                .createAudioDeviceModule()
            val f = PeerConnectionFactory.builder().setAudioDeviceModule(adm).createPeerConnectionFactory()
            nhaMayChung = f
            return f
        }
    }

    private val thongKe = object : Runnable {
        override fun run() {
            val p = pc ?: return
            p.getStats { bao ->
                var rtt: Double? = null
                var rttCap: Double? = null
                var mat: Double? = null
                var byteGui: Double? = null
                for (s in bao.statsMap.values) {
                    val m = s.members
                    when (s.type) {
                        // RTT đo bằng RTCP (báo cáo nhận của máy chủ) – chính xác nhất
                        "remote-inbound-rtp" -> rtt = (m["roundTripTime"] as? Number)?.toDouble() ?: rtt
                        // dự phòng: RTT của cặp ứng viên ICE đang dùng (đo bằng gói STUN kiểm tra kết nối)
                        "candidate-pair" -> if (m["nominated"] == true && m["state"] == "succeeded")
                            rttCap = (m["currentRoundTripTime"] as? Number)?.toDouble()
                        // mất gói chiều nhận: số gói mất / (nhận + mất)
                        "inbound-rtp" -> if (m["kind"] == "audio") {
                            val nhan = (m["packetsReceived"] as? Number)?.toDouble() ?: 0.0
                            val matGoi = (m["packetsLost"] as? Number)?.toDouble() ?: 0.0
                            if (nhan + matGoi > 0) mat = 100.0 * matGoi.coerceAtLeast(0.0) / (nhan + matGoi)
                        }
                        "outbound-rtp" -> if (m["kind"] == "audio") byteGui = (m["bytesSent"] as? Number)?.toDouble()
                    }
                }
                val rttMs = (rtt ?: rttCap)?.let { it * 1000 }
                val bayGio = System.currentTimeMillis()
                var kbps: Double? = null
                byteGui?.let { b ->
                    if (byteTruoc >= 0 && bayGio > tTruoc) kbps = (b - byteTruoc) * 8 / (bayGio - tTruoc)
                    byteTruoc = b
                    tTruoc = bayGio
                }
                chinh.post {
                    khiThongKe(rttMs, mat, kbps)
                    guiTin(JSONObject().put("loai", "thong_ke").put("ma_cuoc", maCuoc)
                        .put("rtt_ms", rttMs?.let { Math.round(it * 10) / 10.0 } ?: JSONObject.NULL)
                        .put("mat_goi", mat?.let { Math.round(it * 100) / 100.0 } ?: JSONObject.NULL))
                }
            }
            chinh.postDelayed(this, 2000)
        }
    }

    /** Bắt đầu đường âm thanh sau khi cuộc gọi được trả lời. Gọi trên luồng giao diện. */
    fun batDau(maCuoc: String) {
        this.maCuoc = maCuoc
        val f = nhaMay(ctx)
        cheDoCu = am.mode
        am.mode = AudioManager.MODE_IN_COMMUNICATION        // định tuyến âm cho đàm thoại (loa trong, AEC)

        val cauHinh = PeerConnection.RTCConfiguration(emptyList()).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            // Lab LAN: không cần STUN/TURN. Qua Internet phải thêm STUN và TURN (xem README).
        }
        val p = f.createPeerConnection(cauHinh, object : PeerConnection.Observer {
            override fun onSignalingChange(s: PeerConnection.SignalingState?) {}
            override fun onIceConnectionChange(s: PeerConnection.IceConnectionState?) {
                chinh.post { khiTrangThai("ICE: $s") }
            }
            override fun onIceConnectionReceivingChange(b: Boolean) {}
            override fun onIceGatheringChange(s: PeerConnection.IceGatheringState?) {
                if (s == PeerConnection.IceGatheringState.COMPLETE) chinh.post { guiOffer() }
            }
            override fun onIceCandidate(c: IceCandidate?) {}
            override fun onIceCandidatesRemoved(c: Array<out IceCandidate>?) {}
            override fun onAddStream(s: MediaStream?) {}
            override fun onRemoveStream(s: MediaStream?) {}
            override fun onDataChannel(d: DataChannel?) {}
            override fun onRenegotiationNeeded() {}
            override fun onAddTrack(r: RtpReceiver?, s: Array<out MediaStream>?) {
                chinh.post { khiTrangThai("đã nhận âm thanh từ máy chủ") }
            }
            override fun onConnectionChange(s: PeerConnection.PeerConnectionState?) {
                chinh.post { khiTrangThai(if (s == PeerConnection.PeerConnectionState.CONNECTED) "đã kết nối" else "WebRTC: $s") }
            }
        }) ?: run {
            khiTrangThai("không tạo được RTCPeerConnection")
            return
        }
        pc = p
        nguon = f.createAudioSource(MediaConstraints())
        mic = f.createAudioTrack("mic0", nguon).also {
            it.setEnabled(!tatMic && !giuMay)
            p.addTrack(it, listOf("luong0"))
        }
        p.createOffer(object : SdpQuanSat() {
            override fun onCreateSuccess(sdp: SessionDescription?) {
                p.setLocalDescription(object : SdpQuanSat() {
                    override fun onSetSuccess() {
                        // dự phòng: nếu sau 2 s ICE chưa báo COMPLETE thì gửi offer với ứng viên đã có
                        chinh.postDelayed({ guiOffer() }, 2000)
                    }
                }, sdp)
            }
        }, MediaConstraints())
        chinh.postDelayed(thongKe, 2000)
    }

    private fun guiOffer() {
        val p = pc ?: return
        if (daGuiOffer) return
        val sdp = p.localDescription ?: return
        daGuiOffer = true
        guiTin(JSONObject().put("loai", "sdp").put("ma_cuoc", maCuoc)
            .put("sdp", JSONObject().put("type", "offer").put("sdp", sdp.description)))
    }

    /** Nhận answer của máy chủ. */
    fun nhanSdp(sdp: JSONObject) {
        val p = pc ?: return
        val loai = SessionDescription.Type.fromCanonicalForm(sdp.optString("type", "answer"))
        p.setRemoteDescription(object : SdpQuanSat() {}, SessionDescription(loai, sdp.optString("sdp")))
    }

    fun datTatMic(bat: Boolean) {
        tatMic = bat
        mic?.setEnabled(!tatMic && !giuMay)
    }

    fun datGiuMay(bat: Boolean) {
        giuMay = bat
        mic?.setEnabled(!tatMic && !giuMay)
    }

    fun datLoaNgoai(bat: Boolean) {
        if (Build.VERSION.SDK_INT >= 31) {
            if (bat) {
                am.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
                    ?.let { am.setCommunicationDevice(it) }
            } else {
                am.clearCommunicationDevice()
            }
        } else {
            @Suppress("DEPRECATION")
            am.isSpeakerphoneOn = bat
        }
    }

    fun dong() {
        chinh.removeCallbacksAndMessages(null)
        pc?.close()
        pc?.dispose()
        pc = null
        mic?.dispose()
        mic = null
        nguon?.dispose()
        nguon = null
        datLoaNgoai(false)
        am.mode = cheDoCu
    }

    /** SdpObserver mặc định: chỉ cần ghi đè hàm quan tâm. */
    private open class SdpQuanSat : SdpObserver {
        override fun onCreateSuccess(sdp: SessionDescription?) {}
        override fun onSetSuccess() {}
        override fun onCreateFailure(loi: String?) {}
        override fun onSetFailure(loi: String?) {}
    }
}
