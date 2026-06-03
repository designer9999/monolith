export interface TotpQrResult {
  secret: string;
  uri?: string;
  issuer?: string;
  account?: string;
}

function normalizeBase32(value: string): string {
  return value.replace(/[\s-]/g, "").toUpperCase();
}

function parseOtpauthUri(raw: string): TotpQrResult {
  const uri = raw.trim();
  const parsed = new URL(uri);
  if (parsed.protocol !== "otpauth:" || parsed.hostname !== "totp") {
    throw new Error("QR is not a TOTP authenticator setup code.");
  }
  const secret = normalizeBase32(parsed.searchParams.get("secret") ?? "");
  if (!/^[A-Z2-7]+=*$/.test(secret) || secret.length < 8) {
    throw new Error("TOTP setup code does not contain a valid secret.");
  }

  const label = decodeURIComponent(parsed.pathname.replace(/^\/+/, ""));
  const issuer = parsed.searchParams.get("issuer") ?? label.split(":")[0] ?? undefined;
  const account = label.includes(":") ? label.split(":").slice(1).join(":") : label || undefined;
  return {
    secret,
    uri,
    issuer: issuer || undefined,
    account,
  };
}

export function extractTotpSecret(raw: string): TotpQrResult {
  const value = raw.trim();
  if (!value) {
    throw new Error("No TOTP setup code found.");
  }
  if (value.toLowerCase().startsWith("otpauth://")) {
    return parseOtpauthUri(value);
  }

  const secret = normalizeBase32(value);
  if (!/^[A-Z2-7]+=*$/.test(secret) || secret.length < 8) {
    throw new Error("Pasted value is not a TOTP setup code.");
  }
  return { secret };
}

async function blobToImageData(blob: Blob): Promise<ImageData> {
  const bitmap = await createImageBitmap(blob);
  try {
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      throw new Error("Could not prepare QR image.");
    }
    ctx.drawImage(bitmap, 0, 0);
    return ctx.getImageData(0, 0, bitmap.width, bitmap.height);
  } finally {
    bitmap.close();
  }
}

export async function decodeTotpQrImage(blob: Blob): Promise<TotpQrResult> {
  const { default: jsQR } = await import("jsqr");
  const image = await blobToImageData(blob);
  const decoded = jsQR(image.data, image.width, image.height, {
    inversionAttempts: "attemptBoth",
  });
  if (!decoded?.data) {
    throw new Error("No QR code was found in the pasted image.");
  }
  return extractTotpSecret(decoded.data);
}
