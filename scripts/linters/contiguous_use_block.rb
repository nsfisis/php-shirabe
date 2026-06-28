def contiguous_use_block(root_dir, excludes = [])
  pattern = root_dir.join('crates', '**', '*.rs').to_s
  errors = Dir.glob(pattern).sort.flat_map do |path|
    relative = Pathname.new(path).relative_path_from(root_dir).to_s
    next [] if excludes.include?(relative)

    find_split_use_block(path, relative)
  end

  return true if errors.empty?

  puts 'Found blank lines splitting the leading `use` block into sections.'
  puts 'All `use` statements at the top of the file must be contiguous (no blank lines between them):'
  errors.each do |err|
    puts "  #{err}"
  end
  false
end

USE_START_RE = /\A(?:pub(?:\([^)]*\))?\s+)?use\b/

def find_split_use_block(path, relative)
  lines = File.readlines(path)
  errors = []

  i = skip_preamble(lines)
  return [] if i.nil?

  loop do
    i = consume_use_statement(lines, i)
    break if i >= lines.length

    blanks = []
    j = i
    while j < lines.length
      stripped = lines[j].strip
      if stripped.empty?
        blanks << j
        j += 1
      elsif stripped.start_with?('//') || stripped.start_with?('#[')
        j += 1
      else
        break
      end
    end

    if j < lines.length && lines[j].strip =~ USE_START_RE
      blanks.each do |bi|
        errors << "#{relative}:#{bi + 1}: blank line splits the leading `use` block"
      end
      i = j
    else
      break
    end
  end

  errors
end

def skip_preamble(lines)
  lines.each_with_index do |raw, idx|
    stripped = raw.strip
    return idx if stripped =~ USE_START_RE
    next if stripped.empty? || stripped.start_with?('//') || stripped.start_with?('#![') || stripped.start_with?('#[')

    return nil
  end
  nil
end

def consume_use_statement(lines, start_idx)
  brace_depth = 0
  i = start_idx
  while i < lines.length
    line = lines[i]
    brace_depth += line.count('{') - line.count('}')
    done = brace_depth <= 0 && line.rstrip.end_with?(';')
    i += 1
    return i if done
  end
  i
end
