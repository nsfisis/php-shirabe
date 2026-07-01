BANNED_USE_PATHS = %w[
  anyhow::Result
  std::any::Any
  std::io::Read
  std::io::Write
  std::process::Command
].freeze

def no_banned_use(root_dir, excludes = [])
  pattern = root_dir.join('crates', '**', '*.rs').to_s
  errors = Dir.glob(pattern).sort.flat_map do |path|
    relative = Pathname.new(path).relative_path_from(root_dir).to_s
    next [] if excludes.include?(relative)

    find_banned_uses(path, relative)
  end

  return true if errors.empty?

  puts 'Found banned `use` imports.'
  puts 'These items must always be referenced by their fully-qualified path.'
  puts 'For imports to use trait methods, use `as _` (e.g., `use std::io::Write as _;`).'
  errors.each do |err|
    puts "  #{err}"
  end
  false
end

BANNED_USE_START_RE = /\A(?:pub(?:\([^)]*\))?\s+)?use\b/

def find_banned_uses(path, relative)
  errors = []
  lines = File.readlines(path)
  buffer = nil
  start_idx = nil

  lines.each_with_index do |raw, idx|
    code = raw.split('//', 2).first || raw
    stripped = code.strip

    if buffer.nil?
      next unless stripped =~ BANNED_USE_START_RE

      buffer = +''
      start_idx = idx
    end

    buffer << ' ' << stripped
    next unless buffer.include?(';')

    tree = buffer[/\buse\s+(.*?);/m, 1]
    expand_use_tree(tree).each do |full|
      next unless BANNED_USE_PATHS.include?(full)

      errors << "#{relative}:#{start_idx + 1}: `use #{full}` is banned (fully qualify as `#{full}` instead)"
    end

    buffer = nil
  end

  errors.uniq
end

def expand_use_tree(tree)
  return [] if tree.nil?

  tree = tree.strip
  brace = tree.index('{')

  if brace.nil?
    stripped = strip_use_alias(tree)
    return [] if stripped.nil?

    return [stripped].reject(&:empty?)
  end

  prefix = tree[0...brace].sub(/::\s*\z/, '').strip
  inner = tree[(brace + 1)..].sub(/\}\s*\z/, '')

  split_top_level(inner).flat_map do |child|
    expand_use_tree(child).map do |sub|
      if sub.empty? || sub == 'self'
        prefix
      elsif prefix.empty?
        sub
      else
        "#{prefix}::#{sub}"
      end
    end
  end
end

def strip_use_alias(segment)
  return nil if segment =~ /\s+as\s+_\s*\z/

  segment.sub(/\s+as\s+\S+\s*\z/, '').strip
end

def split_top_level(str)
  parts = []
  current = +''
  depth = 0

  str.each_char do |ch|
    case ch
    when '{' then depth += 1; current << ch
    when '}' then depth -= 1; current << ch
    when ','
      if depth.zero?
        parts << current
        current = +''
      else
        current << ch
      end
    else
      current << ch
    end
  end
  parts << current

  parts.map(&:strip).reject(&:empty?)
end
