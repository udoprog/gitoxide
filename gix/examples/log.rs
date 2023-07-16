use clap::Parser;
use gix::{
    date::time::format,
    object::Kind,
    objs::CommitRef,
    traverse::commit::Sorting,
};

fn main() {
    let args = Args::parse_from(gix::env::args_os());
    match run(&args) {
        Ok(()) => {}
        Err(e) => eprintln!("error: {e}"),
    }
}

#[derive(Debug, clap::Parser)]
#[clap(name = "log", about = "git log example", version = option_env!("GITOXIDE_VERSION"))]
struct Args {
    #[clap(name = "dir", long = "git-dir")]
    /// Alternative git directory to use
    git_dir: Option<String>,
    #[clap(short, long)]
    /// Number of commits to return
    count: Option<usize>,
    #[clap(short, long)]
    /// Number of commits to skip
    skip: Option<usize>,
    #[clap(short, long)]
    /// Commits are sorted as they are mentioned in the commit graph.
    breadth_first: bool,
    #[clap(short, long)]
    /// Commits are sorted by their commit time in descending order.
    newest_first: bool,
    #[clap(short, long)]
    /// Reverse the commit sort order
    reverse: bool,
    #[clap(name = "commit")]
    /// The starting commit
    commitish: Option<String>,
    #[clap(name = "path")]
    /// The path interested in log history of
    path: Option<String>,
}

fn run(args: &Args) -> anyhow::Result<()> {
    let repo = gix::discover(
        args.git_dir.as_ref().map_or(".", |s| &s[..])
    )?;
    let object = repo.rev_parse_single(
        args.commitish.as_ref().map_or("HEAD", |s| &s[..])
    )?.object()?;
    let commit = match object.kind {
        Kind::Commit => object.try_into_commit()?,
        _ => anyhow::bail!("not a commit object"),
    };

    // TODO better way to deal with these flags.
    let sorting = if args.breadth_first {
        Sorting::BreadthFirst
    }
    else {  // else if args.newest_first {
        Sorting::ByCommitTimeNewestFirst
    };

    let mut log_iter: Box<dyn Iterator<Item = Result<LogEntryInfo, _>>> = Box::new(repo
        .rev_walk([commit.id])
        .sorting(sorting)
        .all()?
        .filter(|info| info.as_ref()
            // TODO the other implementation can take a sequence of
            // paths - if so it should apply this check for all paths.
            .map_or(true, |info| args.path.as_ref().map_or(true, |path| {
                // TODO should make use of the `git2::DiffOptions`
                // counterpart in gix for a set of files and also to
                // generate diffs.
                // should args.path be provided, check that it is in
                // fact relevant for this commit (it present?)
                let oid = repo.rev_parse_single(
                    format!("{}:{}", info.id, path).as_str()
                ).ok();
                // check via the revspec on the path prefixed by the
                // tree of the current commit vs. commit's every parents
                // and see if all matching, if not, include this entry.
                !info.parent_ids
                    .iter()
                    .all(|id| repo.rev_parse_single(
                        format!("{id}:{path}").as_str()
                    ).ok() == oid)
            }))
        )
        .map(|info| {
            let info = info?;
            let commit = info.object()?;
            let commit_ref = CommitRef::from_bytes(&commit.data)?;
            Ok::<_, anyhow::Error>(LogEntryInfo {
                commit_id: format!("{}", commit.id()),
                parents: info.parent_ids.iter()
                    // probably could have a better way to display this
                    .map(|x| x.to_string()[..7].to_string())
                    .collect(),
                author: format!("{} <{}>",
                    commit_ref.author.name, commit_ref.author.email),
                time: commit_ref.author.time.format(format::DEFAULT),
                message: commit_ref.message.to_string(),
            })
        })
    );
    if args.reverse {
        let mut results = log_iter.collect::<Vec<_>>();
        results.reverse();
        log_iter = Box::new(results.into_iter())
    }
    if let Some(n) = args.skip {
        log_iter = Box::new(log_iter.skip(n));
    }
    if let Some(n) = args.count {
        log_iter = Box::new(log_iter.take(n));
    }
    let mut log_iter = log_iter.peekable();

    while let Some(entry) = log_iter.next() {
        let entry = entry?;
        println!("commit {}", entry.commit_id);
        if entry.parents.len() > 1 {
            println!("Merge: {}", entry.parents.join(" "));
        }
        println!("Author: {}", entry.author);
        println!("Date:   {}\n", entry.time);
        for line in entry.message.lines() {
            println!("    {line}");
        }
        // only include newline if more log entries, mimicking `git log`
        if log_iter.peek().is_some() {
            println!();
        }
    }

    Ok(())
}

pub struct LogEntryInfo {
    pub commit_id: String,
    pub parents: Vec<String>,
    pub author: String,
    pub time: String,
    pub message: String,
}
