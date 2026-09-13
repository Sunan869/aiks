import { Loader2, AlertCircle } from "lucide-react";

interface Props {
  step: string;
  error: string | null;
}

export default function StartupScreen({ step, error }: Props) {
  return (
    <div className="flex flex-col items-center justify-center h-screen bg-gradient-to-br from-slate-900 to-blue-950 text-white">
      <div className="mb-8 text-center">
        <div className="text-5xl font-bold mb-2 bg-gradient-to-r from-blue-400 to-sky-300 bg-clip-text text-transparent">
          AIKS
        </div>
        <div className="text-slate-400 text-sm">AI Knowledge Sync</div>
      </div>

      {error ? (
        <div className="flex flex-col items-center gap-3 text-red-400">
          <AlertCircle className="w-8 h-8" />
          <p className="text-sm text-center max-w-xs">{error}</p>
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3">
          <Loader2 className="w-8 h-8 animate-spin text-blue-400" />
          <p className="text-slate-300 text-sm">{step}</p>
        </div>
      )}
    </div>
  );
}
