import { useMemo, useState, type SVGProps } from 'react';
import { Icon } from '@astryxdesign/core/Icon';
import { AppWindow } from 'lucide-react';

function PendingIcon(props: SVGProps<SVGSVGElement>) {
  return <svg {...props} viewBox="0 0 32 32" />;
}

export function ProcessIcon({ dataUrl }: { dataUrl: string | null | undefined }) {
  const [failedUrl, setFailedUrl] = useState<string>();
  // Astryx accepts an SVG component; keep its identity stable while the URL is unchanged.
  const glyph = useMemo(() => function ApplicationIcon(props: SVGProps<SVGSVGElement>) {
    return <svg {...props} viewBox="0 0 32 32"><image href={dataUrl ?? undefined} width="32" height="32" onError={() => { if (dataUrl) setFailedUrl(dataUrl); }} /></svg>;
  }, [dataUrl]);
  if (dataUrl === undefined) return <Icon icon={PendingIcon} size="lg" />;
  if (dataUrl === null || dataUrl === failedUrl) return <Icon icon={AppWindow} color="secondary" size="lg" />;
  return <Icon icon={glyph} size="lg" />;
}
