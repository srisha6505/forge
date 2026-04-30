// Icons barrel — re-exports lucide-react icons used across Forge UI.
// Use this instead of importing directly from lucide-react so the icon
// set is explicit and swappable.
//
// Note: this codebase currently pins lucide-react ^1.8.0, which renamed
// two icons from the names used by the prototype:
//   MoreHorizontal -> Ellipsis        (aliased below)
//   AlignLeft      -> TextAlignStart  (aliased below)
// The aliased names mirror the prototype so downstream code reads the
// same.

export {
  Files,
  Search,
  MessageSquare,
  GitFork as Network,
  Terminal,
  Settings,
  Mic,
  Moon,
  Sun,
  ChevronRight,
  ChevronDown,
  ChevronUp,
  X,
  Plus,
  File,
  FileText,
  Folder,
  FolderOpen,
  Check,
  Copy,
  RotateCcw,
  ExternalLink,
  Ellipsis as MoreHorizontal,
  Send,
  PanelRight,
  User,
  Bot,
  Sparkles,
  Zap,
  Download,
  Trash2,
  Key,
  Globe,
  SlidersHorizontal,
  Wrench,
  Keyboard,
  Info,
  PenLine,
  BookOpen,
  Eye,
  ListTree,
  History,
  Link2,
  MessageSquarePlus,
  ArrowUpRight,
  TextAlignStart as AlignLeft,
} from "lucide-react";
