def no_decorative_section_comment(root_dir, excludes = [])
  pattern = root_dir.join('crates', '**', '*.rs').to_s
  errors = Dir.glob(pattern).sort.flat_map do |path|
    relative = Pathname.new(path).relative_path_from(root_dir).to_s
    next [] if excludes.include?(relative)

    find_decorative_comments(path, relative)
  end

  return true if errors.empty?

  puts 'Found decorative section comments (4+ consecutive `=`, `-`, or Unicode box-drawing characters).'
  puts 'These section dividers are unnecessarily noisy — remove them:'
  errors.each do |err|
    puts "  #{err}"
  end
  false
end

DECORATIVE_RUN_RE = /[-=]{4,}|[─-╿]{4,}/

def find_decorative_comments(path, relative)
  errors = []

  File.readlines(path).each_with_index do |raw, idx|
    stripped = raw.lstrip
    next unless stripped.start_with?('//')
    next if stripped.start_with?('///') || stripped.start_with?('//!')

    next unless stripped.match?(DECORATIVE_RUN_RE)

    errors << "#{relative}:#{idx + 1}: decorative section comment"
  end

  errors
end
