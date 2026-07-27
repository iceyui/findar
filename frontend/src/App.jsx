import { useEffect, useMemo, useRef, useState } from "react";
import { isSupabaseConfigured, supabase } from "./supabaseClient";

const DEFAULT_TOLERANCE = 500;
const TARGETS_STORAGE_KEY = "ar-bbn.targets";

const joinUrl = (base, path) => {
  if (!path) return "";
  if (!base) return path;
  if (base.endsWith("/") && path.startsWith("/")) return base.slice(0, -1) + path;
  if (!base.endsWith("/") && !path.startsWith("/")) return `${base}/${path}`;
  return base + path;
};

const formatNumber = (value) =>
  new Intl.NumberFormat("id-ID").format(Number(value || 0));

const formatDigits = (value) => {
  const digits = String(value || "").replace(/\D/g, "");
  if (!digits) return "";
  return formatNumber(digits);
};

const normalizeTargets = (value) => {
  return String(value || "")
    .split(",")
    .map((part) => part.replace(/\D/g, ""))
    .filter((part) => part.length > 0)
    .join(",");
};

const buildTokensFromRaw = (rawValue) => {
  const normalized = normalizeTargets(rawValue);
  if (!normalized) return [];
  return normalized.split(",").map((value) => ({
    id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
    value
  }));
};

const positionAfterDigits = (value, digitsCount) => {
  if (digitsCount <= 0) return 0;
  let count = 0;
  for (let i = 0; i < value.length; i += 1) {
    if (/\d/.test(value[i])) {
      count += 1;
      if (count === digitsCount) {
        return i + 1;
      }
    }
  }
  return value.length;
};

const formatTargetsDisplay = (rawValue) => {
  const raw = String(rawValue || "");
  const hasTrailingComma = /,\s*$/.test(raw);
  const parts = raw.split(",");
  const formattedParts = parts.map((part) => formatDigits(part));

  let formatted = formattedParts.join(", ");
  if (hasTrailingComma) {
    if (formatted && !formatted.endsWith(", ")) {
      formatted += ", ";
    } else if (!formatted) {
      formatted = ", ";
    }
  }
  return formatted.trimStart();
};

const countDigits = (value) => (value.match(/\d/g) || []).length;

const insertCommaAtDigits = (rawValue, digitsBefore) => {
  const cleaned = String(rawValue || "").replace(/[^\d,]/g, "");
  if (digitsBefore <= 0) {
    return cleaned.startsWith(",") ? cleaned : `,${cleaned}`;
  }
  let count = 0;
  for (let i = 0; i < cleaned.length; i += 1) {
    if (/\d/.test(cleaned[i])) {
      count += 1;
      if (count === digitsBefore) {
        if (cleaned[i + 1] === ",") {
          return cleaned;
        }
        return `${cleaned.slice(0, i + 1)},${cleaned.slice(i + 1)}`;
      }
    }
  }
  return cleaned.endsWith(",") ? cleaned : `${cleaned},`;
};

const removeCommaAtIndex = (rawValue, commaIndex) => {
  if (commaIndex < 0) return rawValue;
  let count = 0;
  let out = "";
  for (let i = 0; i < rawValue.length; i += 1) {
    const ch = rawValue[i];
    if (ch === ",") {
      if (count === commaIndex) {
        count += 1;
        continue;
      }
      count += 1;
    }
    out += ch;
  }
  return out;
};

const removeDigitFromRaw = (rawValue, displayValue, cursorIndex, direction) => {
  const tokens = String(rawValue || "").split(",");
  const displayTokens = String(displayValue || "").split(",");
  const digitsPerToken = displayTokens.map((part) =>
    part.replace(/\D/g, "")
  );

  const before = String(displayValue || "").slice(0, cursorIndex);
  let tokenIndex = (before.match(/,/g) || []).length;
  let digitsBefore = countDigits(before.split(",").pop() || "");

  if (direction === "back") {
    if (digitsBefore > 0) {
      const token = digitsPerToken[tokenIndex] || "";
      digitsPerToken[tokenIndex] =
        token.slice(0, digitsBefore - 1) + token.slice(digitsBefore);
    } else if (tokenIndex > 0) {
      tokenIndex -= 1;
      const token = digitsPerToken[tokenIndex] || "";
      if (token.length > 0) {
        digitsPerToken[tokenIndex] = token.slice(0, -1);
      }
    }
  } else {
    const token = digitsPerToken[tokenIndex] || "";
    if (digitsBefore < token.length) {
      digitsPerToken[tokenIndex] =
        token.slice(0, digitsBefore) + token.slice(digitsBefore + 1);
    } else if (tokenIndex + 1 < digitsPerToken.length) {
      const next = digitsPerToken[tokenIndex + 1] || "";
      if (next.length > 0) {
        digitsPerToken[tokenIndex + 1] = next.slice(1);
      }
    }
  }

  const hasTrailingComma = /,\s*$/.test(displayValue || "");
  let rebuilt = digitsPerToken.join(",");
  if (hasTrailingComma && !rebuilt.endsWith(",")) {
    rebuilt += ",";
  }
  return rebuilt;
};

const turnstileSiteKey = import.meta.env.VITE_TURNSTILE_SITE_KEY || "";
const isTurnstileConfigured = Boolean(turnstileSiteKey);

export default function App() {
  const [session, setSession] = useState(null);
  const [authLoading, setAuthLoading] = useState(true);
  const [authSubmitting, setAuthSubmitting] = useState(false);
  const [authEmail, setAuthEmail] = useState("");
  const [authPassword, setAuthPassword] = useState("");
  const [authError, setAuthError] = useState("");
  const [turnstileReady, setTurnstileReady] = useState(false);
  const [turnstileToken, setTurnstileToken] = useState("");
  const [turnstileRender, setTurnstileRender] = useState(0);
  const turnstileRef = useRef(null);
  const turnstileWidgetId = useRef(null);
  const [file, setFile] = useState(null);
  const [targetsRaw, setTargetsRaw] = useState("");
  const [targetInput, setTargetInput] = useState("");
  const [targetTokens, setTargetTokens] = useState([]);
  const [tolerance, setTolerance] = useState("");
  const [backendStatus, setBackendStatus] = useState("checking");
  const [backendLatency, setBackendLatency] = useState(null);
  const [loading, setLoading] = useState(false);
  const [downloadLoading, setDownloadLoading] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState(null);
  const fileInputRef = useRef(null);
  const targetsInputRef = useRef(null);

  const apiBase = import.meta.env.VITE_API_BASE || "http://localhost:8000";

  useEffect(() => {
    if (!supabase) {
      setAuthLoading(false);
      return undefined;
    }

    let active = true;

    supabase.auth.getSession().then(({ data }) => {
      if (active) {
        setSession(data.session);
        setAuthLoading(false);
      }
    });

    const {
      data: { subscription }
    } = supabase.auth.onAuthStateChange((_event, nextSession) => {
      setSession(nextSession);
      setAuthLoading(false);
    });

    return () => {
      active = false;
      subscription.unsubscribe();
    };
  }, []);

  useEffect(() => {
    try {
      const saved = window.localStorage.getItem(TARGETS_STORAGE_KEY);
      if (!saved) return;
      const parsed = JSON.parse(saved);
      const savedRaw = typeof parsed.targetsRaw === "string" ? parsed.targetsRaw : "";
      const savedInput =
        typeof parsed.targetInput === "string" ? parsed.targetInput : "";
      if (savedRaw) {
        setTargetsRaw(savedRaw);
        setTargetTokens(buildTokensFromRaw(savedRaw));
      }
      if (savedInput) {
        setTargetInput(savedInput);
      }
    } catch (err) {
      window.localStorage.removeItem(TARGETS_STORAGE_KEY);
    }
  }, []);

  useEffect(() => {
    if (!targetsRaw && !targetInput) {
      window.localStorage.removeItem(TARGETS_STORAGE_KEY);
      return;
    }
    const payload = JSON.stringify({
      targetsRaw,
      targetInput
    });
    window.localStorage.setItem(TARGETS_STORAGE_KEY, payload);
  }, [targetsRaw, targetInput]);

  useEffect(() => {
    let active = true;

    const checkBackend = async () => {
      const controller = new AbortController();
      const timeoutId = window.setTimeout(() => controller.abort(), 3000);
      const startedAt = performance.now();
      try {
        const response = await fetch(joinUrl(apiBase, "/api/health"), {
          signal: controller.signal
        });
        const endedAt = performance.now();
        if (!active) return;
        if (!response.ok) {
          setBackendStatus("offline");
          setBackendLatency(null);
          return;
        }
        setBackendStatus("online");
        setBackendLatency(Math.max(1, Math.round(endedAt - startedAt)));
      } catch (err) {
        if (!active) return;
        setBackendStatus("offline");
        setBackendLatency(null);
      } finally {
        window.clearTimeout(timeoutId);
      }
    };

    checkBackend();
    const intervalId = window.setInterval(checkBackend, 10000);
    return () => {
      active = false;
      window.clearInterval(intervalId);
    };
  }, [apiBase]);

  useEffect(() => {
    if (!isTurnstileConfigured) return;

    let script = document.querySelector('script[src*="turnstile"]');
    if (!script) {
      script = document.createElement("script");
      script.src = "https://challenges.cloudflare.com/turnstile/v0/api.js?onload=onTurnstileLoad";
      script.async = true;
      script.defer = true;
      document.head.appendChild(script);
    }

    window.onTurnstileLoad = () => {
      setTurnstileReady(true);
    };

    return () => {
      if (turnstileWidgetId.current !== null) {
        window.turnstile?.remove(turnstileWidgetId.current);
        turnstileWidgetId.current = null;
      }
    };
  }, []);

  useEffect(() => {
    if (!turnstileReady || !turnstileRef.current) return;

    if (turnstileWidgetId.current !== null) {
      window.turnstile?.remove(turnstileWidgetId.current);
      turnstileWidgetId.current = null;
    }

    turnstileWidgetId.current = window.turnstile.render(turnstileRef.current, {
      sitekey: turnstileSiteKey,
      callback: (token) => {
        setTurnstileToken(token);
      },
      "expired-callback": () => {
        setTurnstileToken("");
      },
    });
  }, [turnstileReady, turnstileRender]);

  const clearFile = () => {
    setFile(null);
    if (fileInputRef.current) {
      fileInputRef.current.value = "";
    }
  };

  const getAuthHeaders = () => {
    if (!session?.access_token) return {};
    return {
      Authorization: `Bearer ${session.access_token}`
    };
  };

  const canSubmit = useMemo(() => {
    const normalized = normalizeTargets([targetsRaw, targetInput].filter(Boolean).join(","));
    return file && normalized && Number(normalized.split(",")[0]) > 0;
  }, [file, targetsRaw, targetInput]);

  const backendLabel = useMemo(() => {
    if (backendStatus === "checking") {
      return "Mengecek backend...";
    }
    if (backendStatus === "offline") {
      return "Backend nonaktif";
    }
    if (backendLatency) {
      return `Backend aktif - ${backendLatency}ms`;
    }
    return "Backend aktif";
  }, [backendStatus, backendLatency]);

  const commitTargets = (rawValue) => {
    const parts = String(rawValue || "")
      .split(",")
      .map((part) => part.replace(/\D/g, ""))
      .filter((part) => part.length > 0);
    if (parts.length === 0) return;
    const newTokens = parts.map((value) => ({
      id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
      value
    }));
    setTargetTokens((prev) => [...prev, ...newTokens]);
    setTargetsRaw((prev) => {
      const prefix = prev ? `${prev},` : "";
      return `${prefix}${parts.join(",")}`;
    });
    setTargetInput("");
  };

  const removeTargetById = (id) => {
    setTargetTokens((prev) => {
      const next = prev.filter((token) => token.id !== id);
      setTargetsRaw(next.map((token) => token.value).join(","));
      return next;
    });
  };

  const removeLastTarget = () => {
    setTargetTokens((prev) => {
      if (prev.length === 0) return prev;
      const next = prev.slice(0, -1);
      setTargetsRaw(next.map((token) => token.value).join(","));
      return next;
    });
  };

  const handleSubmit = async (event) => {
    event.preventDefault();
    setError("");
    setResult(null);

    if (!session?.access_token) {
      setError("Sesi login tidak valid. Silakan login ulang.");
      return;
    }

    const normalizedTargets = normalizeTargets(
      [targetsRaw, targetInput].filter(Boolean).join(",")
    );
    if (!file || !normalizedTargets) {
      setError("Silakan pilih file dan isi target nominal.");
      return;
    }

    const formData = new FormData();
    formData.append("file", file);
    formData.append("targets", normalizedTargets);
    const normalizedTolerance = tolerance
      ? tolerance.replace(/\D/g, "")
      : "";
    if (normalizedTolerance) {
      formData.append("tolerance", normalizedTolerance);
    } else {
      formData.append("tolerance", String(DEFAULT_TOLERANCE));
    }

    setLoading(true);
    try {
      const response = await fetch(joinUrl(apiBase, "/api/process"), {
        method: "POST",
        headers: getAuthHeaders(),
        body: formData
      });

      const data = await response.json();
      if (!response.ok) {
        throw new Error(data.detail || "Request failed");
      }

      setResult(data);
    } catch (err) {
      setError(err.message || "Something went wrong");
    } finally {
      setLoading(false);
      setFile(null);
      setTargetsRaw("");
      setTargetInput("");
      setTargetTokens([]);
      setTolerance("");
      if (fileInputRef.current) {
        fileInputRef.current.value = "";
      }
    }
  };

  const handleLogin = async (event) => {
    event.preventDefault();
    setAuthError("");

    if (!supabase) {
      setAuthError("Supabase Auth belum dikonfigurasi.");
      return;
    }

    if (isTurnstileConfigured && !turnstileToken) {
      setAuthError("Verifikasi Turnstile diperlukan.");
      return;
    }

    setAuthSubmitting(true);
    try {
      if (isTurnstileConfigured) {
        const verifyResp = await fetch(joinUrl(apiBase, "/api/verify-turnstile"), {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ token: turnstileToken }),
        });
        const verifyData = await verifyResp.json();
        if (!verifyResp.ok || !verifyData.success) {
          throw new Error("Verifikasi Turnstile gagal. Coba refresh halaman.");
        }
      }

      const { error: signInError } = await supabase.auth.signInWithPassword({
        email: authEmail.trim(),
        password: authPassword
      });
      if (signInError) {
        throw signInError;
      }
      setAuthPassword("");
    } catch (err) {
      setAuthError(err.message || "Login gagal.");
    } finally {
      setAuthSubmitting(false);
      setTurnstileToken("");
      if (turnstileWidgetId.current !== null) {
        window.turnstile?.remove(turnstileWidgetId.current);
        turnstileWidgetId.current = null;
      }
      setTurnstileRender((k) => k + 1);
    }
  };

  const handleLogout = async () => {
    if (supabase) {
      await supabase.auth.signOut();
    }
    setFile(null);
    setTargetsRaw("");
    setTargetInput("");
    setTargetTokens([]);
    setTolerance("");
    setResult(null);
    setError("");
    setTurnstileToken("");
    if (turnstileWidgetId.current !== null) {
      window.turnstile?.remove(turnstileWidgetId.current);
      turnstileWidgetId.current = null;
    }
    setTurnstileRender((k) => k + 1);
  };

  const handleDownload = async () => {
    if (!result?.download_url || !session?.access_token) return;

    setDownloadLoading(true);
    setError("");
    try {
      const dataUrl = result.download_url.replace("/api/download/", "/api/download-data/");
      const response = await fetch(joinUrl(apiBase, dataUrl), {
        headers: getAuthHeaders()
      });
      if (!response.ok) {
        const data = await response.json().catch(() => ({}));
        throw new Error(data.detail || "Gagal mengunduh file hasil.");
      }

      const payload = await response.json();
      const binary = atob(payload.data);
      const bytes = new Uint8Array(binary.length);
      for (let index = 0; index < binary.length; index += 1) {
        bytes[index] = binary.charCodeAt(index);
      }
      const blob = new Blob([bytes], {
        type:
          payload.media_type ||
          "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
      });
      const downloadUrl = window.URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = downloadUrl;
      anchor.download = payload.file_name || result.file_name || "hasil_piutang.xlsx";
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      window.URL.revokeObjectURL(downloadUrl);
    } catch (err) {
      setError(err.message || "Gagal mengunduh file hasil.");
    } finally {
      setDownloadLoading(false);
    }
  };

  if (!isSupabaseConfigured) {
    return (
      <div className="page">
        <div className="background" />
        <main className="auth-shell">
          <section className="auth-card">
            <p className="brand-mark">
              <img src="/vanilla-bean-svgrepo-com.svg" alt="" aria-hidden="true" />
              <span>AR Vanila</span>
            </p>
            <h1>Supabase Auth belum dikonfigurasi.</h1>
            <p>
              Isi `VITE_SUPABASE_URL` dan `VITE_SUPABASE_PUBLISHABLE_KEY` di
              file environment frontend.
            </p>
          </section>
        </main>
      </div>
    );
  }

  if (authLoading) {
    return (
      <div className="page">
        <div className="background" />
        <main className="auth-shell">
          <section className="auth-card auth-card--compact">
            <span className="loading-spinner" aria-hidden="true" />
            <strong>Memeriksa sesi login</strong>
          </section>
        </main>
      </div>
    );
  }

  if (!session) {
    return (
      <div className="page">
        <div className="background" />
        <main className="auth-shell">
          <section className="auth-card">
            <p className="brand-mark">
              <img src="/vanilla-bean-svgrepo-com.svg" alt="" aria-hidden="true" />
              <span>AR Vanila</span>
            </p>
            <h1>Login internal</h1>
            <form className="auth-form" onSubmit={handleLogin}>
              <label className="field">
                <span className="label">Email</span>
                <input
                  type="email"
                  autoComplete="email"
                  value={authEmail}
                  onChange={(event) => setAuthEmail(event.target.value)}
                  required
                />
              </label>
              <label className="field">
                <span className="label">Password</span>
                <input
                  type="password"
                  autoComplete="current-password"
                  value={authPassword}
                  onChange={(event) => setAuthPassword(event.target.value)}
                  required
                />
              </label>
              {isTurnstileConfigured ? <div ref={turnstileRef} className="turnstile-wrap" /> : null}
              {authError ? <p className="error auth-error">{authError}</p> : null}
              <button className="primary" type="submit" disabled={authSubmitting}>
                {authSubmitting ? "Memproses..." : "Login"}
              </button>
            </form>
          </section>
        </main>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="background" />
      {loading ? (
        <div
          className="loading-overlay"
          role="dialog"
          aria-modal="true"
          aria-live="polite"
        >
          <div className="loading-card">
            <span className="loading-spinner" aria-hidden="true" />
            <div className="loading-text">
              <strong>Sedang menghitung</strong>
              <span>Mohon ditunggu, proses bisa memakan waktu.</span>
            </div>
          </div>
        </div>
      ) : null}
      <main className="container">
        <header className="hero">
          <p className="brand-mark">
            <img src="/vanilla-bean-svgrepo-com.svg" alt="" aria-hidden="true" />
            <span>AR Vanila</span>
          </p>
          <div className="hero-meta">
            <span className={`status-pill status-${backendStatus}`}>
              <span className="status-dot" aria-hidden="true" />
              {backendLabel}
            </span>
            <button className="session-button" type="button" onClick={handleLogout}>
              Logout
            </button>
          </div>
          <h1>Temukan piutang toko dari nominal pembayaran.</h1>
          <p className="subtitle">
            Unggah Excel, masukkan nominal pembayaran toko, lalu sistem
            mencocokkan kombinasi piutang yang paling mendekati target.
          </p>
        </header>

        <section className="card">
          <form className="form" onSubmit={handleSubmit}>
            <fieldset className="fieldset" disabled={loading}>
            <div className="field">
              <label className="label" htmlFor="file-input">
                File Excel (.xlsx)
              </label>
              <span className="helper">
                Unggah file piutang dalam format .xlsx.
              </span>
              {file ? (
                <div
                  className="file-status"
                  onClick={() => {
                    if (fileInputRef.current) {
                      fileInputRef.current.click();
                    }
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      fileInputRef.current?.click();
                    }
                  }}
                  role="button"
                  tabIndex={0}
                >
                  <div className="file-status__left">
                    <div className="file-icon" aria-hidden="true">
                      <svg viewBox="0 0 24 24" role="img">
                        <path
                          d="M6 3h7l5 5v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zm7 1.5V9h4.5"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="1.6"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                        <path
                          d="M8 13h8M8 16h8M8 19h5"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="1.6"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                      </svg>
                    </div>
                    <span className="file-status__name">{file.name}</span>
                  </div>
                  <button
                    type="button"
                    className="file-remove"
                    aria-label="Hapus file"
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      clearFile();
                    }}
                  >
                    x
                  </button>
                </div>
              ) : (
                <div className="file-input">
                  <div className="file-input__inner">
                    <div className="file-icon" aria-hidden="true">
                      <svg viewBox="0 0 24 24" role="img">
                        <path
                          d="M6 3h7l5 5v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zm7 1.5V9h4.5"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="1.6"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                        <path
                          d="M8 13h8M8 16h8M8 19h5"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="1.6"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                      </svg>
                    </div>
                    <div className="file-text">
                      <span className="file-title">Pilih file Excel</span>
                      <span className="file-subtitle">Format .xlsx, maksimal 10MB</span>
                    </div>
                  </div>
                  <input
                    id="file-input"
                    ref={fileInputRef}
                    type="file"
                    accept=".xlsx"
                    onChange={(event) => {
                      const selected = event.target.files?.[0] || null;
                      if (selected) {
                        setFile(selected);
                      }
                    }}
                  />
                </div>
              )}
            </div>

            <label className="field">
              <span className="label">Nominal pembayaran toko</span>
              <span className="helper">Pisahkan lebih dari satu nominal dengan koma.</span>
              <div
                className="target-input"
                onClick={() => targetsInputRef.current?.focus()}
                onBlur={(event) => {
                  if (!event.currentTarget.contains(event.relatedTarget)) {
                    commitTargets(targetInput);
                  }
                }}
              >
                {targetTokens.map((token) => (
                  <span
                    className="target-chip"
                    key={token.id}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      targetsInputRef.current?.focus();
                    }}
                  >
                    {formatDigits(token.value)}
                      <button
                        type="button"
                        className="target-remove"
                        aria-label="Hapus nominal"
                        onClick={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          removeTargetById(token.id);
                        }}
                      >
                        x
                      </button>
                    </span>
                ))}
                <input
                  ref={targetsInputRef}
                  className="target-input__field"
                  type="text"
                  inputMode="numeric"
                  placeholder={
                    targetTokens.length === 0 && !targetInput
                      ? "Contoh: 2.300.000, 4.150.000"
                      : ""
                  }
                  value={formatDigits(targetInput)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === ",") {
                      event.preventDefault();
                      commitTargets(targetInput);
                      return;
                    }
                    if (event.key === "Backspace" && !targetInput) {
                      event.preventDefault();
                      removeLastTarget();
                    }
                  }}
                  onChange={(event) => {
                    const cleaned = event.target.value.replace(/\D/g, "");
                    setTargetInput(cleaned);
                  }}
                  onPaste={(event) => {
                    const text = event.clipboardData.getData("text");
                    if (text.includes(",")) {
                      event.preventDefault();
                      commitTargets(text);
                      return;
                    }
                    const cleaned = text.replace(/\D/g, "");
                    setTargetInput(cleaned);
                  }}
                />
              </div>
            </label>

            <label className="field">
              <span className="label">Toleransi (opsional)</span>
              <span className="helper">Default {formatNumber(DEFAULT_TOLERANCE)}.</span>
              <input
                type="text"
                inputMode="numeric"
                placeholder={`Default: ${formatNumber(DEFAULT_TOLERANCE)}`}
                value={tolerance}
                onChange={(event) => setTolerance(formatDigits(event.target.value))}
              />
            </label>

            <div className="actions">
              <button className="primary" type="submit" disabled={!canSubmit || loading}>
                {loading ? (
                  <>
                    <span className="spinner" aria-hidden="true" />
                    Memproses...
                  </>
                ) : (
                  "Cari piutang"
                )}
              </button>
            </div>
            </fieldset>
          </form>

          {error ? <p className="error">{error}</p> : null}

          {result ? (
            <div className="result-card">
              {result.found ? (
                <div className="result-row">
                  <div className="result-status">
                    <span className="result-label">Ditemukan</span>
                    <strong className="result-count">{result.total_rows}</strong>
                    <span className="result-label">baris cocok.</span>
                  </div>
                  <button
                    type="button"
                    className="download"
                    onClick={handleDownload}
                    disabled={downloadLoading}
                  >
                    <span className="download-icon" aria-hidden="true">
                      <svg viewBox="0 0 24 24" role="img">
                        <path
                          d="M12 4v10m0 0l4-4m-4 4l-4-4"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="1.8"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                        />
                        <path
                          d="M5 20h14"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="1.8"
                          strokeLinecap="round"
                        />
                      </svg>
                    </span>
                    {downloadLoading ? "Mengunduh..." : "Unduh file hasil"}
                  </button>
                </div>
              ) : (
                <p className="result-empty">Tidak ada kombinasi yang cocok.</p>
              )}
            </div>
          ) : null}
        </section>

        <section className="note">
          <h2>Alur kerja</h2>
          <ol className="steps">
            <li>Upload file Excel.</li>
            <li>
              Isi nominal pembayaran toko (bisa lebih dari satu).
            </li>
            <li>Isi toleransi (opsional).</li>
            <li>Klik proses, lalu download hasilnya.</li>
          </ol>
        </section>
        <footer className="footer">
          <div>AR Vanila – ar.vanila.id</div>
          <div>Sistem internal untuk pencocokan piutang toko</div>
        </footer>
      </main>
    </div>
  );
}
