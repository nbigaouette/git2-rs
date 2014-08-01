use std::c_str::CString;
use std::kinds::marker;
use libc::{c_int, c_uint};

use {raw, Revspec, Error, doit, init, Object, RepositoryState};

pub struct Repository {
    raw: *mut raw::git_repository,
    marker1: marker::NoShare,
    marker2: marker::NoSend,
}

impl Repository {
    /// Attempt to open an already-existing repository at `path`.
    ///
    /// The path can point to either a normal or bare repository.
    pub fn open(path: &Path) -> Result<Repository, Error> {
        init();
        let s = path.to_c_str();
        let mut ret = 0 as *mut raw::git_repository;
        try!(doit(|| unsafe {
            raw::git_repository_open(&mut ret, s.as_ptr())
        }));
        Ok(Repository {
            raw: ret,
            marker1: marker::NoShare,
            marker2: marker::NoSend,
        })
    }

    /// Creates a new repository in the specified folder.
    ///
    /// The folder must exist prior to invoking this function.
    pub fn init(path: &Path, bare: bool) -> Result<Repository, Error> {
        init();
        let s = path.to_c_str();
        let mut ret = 0 as *mut raw::git_repository;
        try!(doit(|| unsafe {
            raw::git_repository_init(&mut ret, s.as_ptr(), bare as c_uint)
        }));
        Ok(Repository {
            raw: ret,
            marker1: marker::NoShare,
            marker2: marker::NoSend,
        })
    }

    /// Execute a rev-parse operation against the `spec` listed.
    ///
    /// The resulting revision specification is returned, or an error is
    /// returned if one occurs.
    pub fn revparse(&self, spec: &str) -> Result<Revspec, Error> {
        let s = spec.to_c_str();
        let mut spec = raw::git_revspec {
            from: 0 as *mut _,
            to: 0 as *mut _,
            flags: raw::git_revparse_mode_t::empty(),
        };
        try!(doit(|| unsafe {
            raw::git_revparse(&mut spec, self.raw, s.as_ptr())
        }));

        if spec.flags.contains(raw::GIT_REVPARSE_SINGLE) {
            assert!(spec.to.is_null());
            let obj = unsafe { Object::from_raw(self, spec.from) };
            Ok(Revspec::from_objects(Some(obj), None))
        } else {
            fail!()
        }
    }

    /// Find a single object, as specified by a revision string.
    pub fn revparse_single(&self, spec: &str) -> Result<Object, Error> {
        let s = spec.to_c_str();
        let mut obj = 0 as *mut raw::git_object;
        try!(doit(|| unsafe {
            raw::git_revparse_single(&mut obj, self.raw, s.as_ptr())
        }));
        assert!(!obj.is_null());
        Ok(unsafe { Object::from_raw(self, obj) })
    }

    /// Tests whether this repository is a bare repository or not.
    pub fn is_bare(&self) -> bool {
        unsafe { raw::git_repository_is_bare(self.raw) == 1 }
    }

    /// Tests whether this repository is a shallow clone.
    pub fn is_shallow(&self) -> bool {
        unsafe { raw::git_repository_is_shallow(self.raw) == 1 }
    }

    /// Tests whether this repository is empty.
    pub fn is_empty(&self) -> Result<bool, Error> {
        let empty = try!(doit(|| unsafe {
            raw::git_repository_is_empty(self.raw)
        }));
        Ok(empty == 1)
    }

    /// Returns the path to the `.git` folder for normal repositories or the
    /// repository itself for bare repositories.
    pub fn path(&self) -> Path {
        unsafe {
            let ptr = raw::git_repository_path(self.raw);
            assert!(!ptr.is_null());
            Path::new(CString::new(ptr, false).as_bytes_no_nul())
        }
    }

    /// Returns the current state of this repository
    pub fn state(&self) -> RepositoryState {
        let state = unsafe { raw::git_repository_state(self.raw) };
        macro_rules! check( ($($raw:ident => $real:ident),*) => (
            $(if state == raw::$raw as c_int { super::$real }) else *
            else {
                fail!("unknown repository state: {}", state)
            }
        ) )

        check!(
            GIT_REPOSITORY_STATE_NONE => Clean,
            GIT_REPOSITORY_STATE_MERGE => Merge,
            GIT_REPOSITORY_STATE_REVERT => Revert,
            GIT_REPOSITORY_STATE_CHERRYPICK => CherryPick,
            GIT_REPOSITORY_STATE_BISECT => Bisect,
            GIT_REPOSITORY_STATE_REBASE => Rebase,
            GIT_REPOSITORY_STATE_REBASE_INTERACTIVE => RebaseInteractive,
            GIT_REPOSITORY_STATE_REBASE_MERGE => RebaseMerge,
            GIT_REPOSITORY_STATE_APPLY_MAILBOX => ApplyMailbox,
            GIT_REPOSITORY_STATE_APPLY_MAILBOX_OR_REBASE => ApplyMailboxOrRebase
        )
    }

    /// Get the path of the working directory for this repository.
    ///
    /// If this repository is bare, then `None` is returned.
    pub fn workdir(&self) -> Option<Path> {
        unsafe {
            let ptr = raw::git_repository_workdir(self.raw);
            if ptr.is_null() {
                None
            } else {
                Some(Path::new(CString::new(ptr, false).as_bytes_no_nul()))
            }
        }
    }
}

#[unsafe_destructor]
impl Drop for Repository {
    fn drop(&mut self) {
        unsafe { raw::git_repository_free(self.raw) }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{TempDir, Command, File};
    use std::str;

    use super::Repository;

    macro_rules! git( ( $cwd:expr, $($arg:expr),*) => ({
        let out = Command::new("git").cwd($cwd) $(.arg($arg))* .output().unwrap();
        assert!(out.status.success());
        str::from_utf8(out.output.as_slice()).unwrap().trim().to_string()
    }) )

    #[test]
    fn smoke_init() {
        let td = TempDir::new("test").unwrap();
        let path = td.path();

        let repo = Repository::init(path, false).unwrap();
        assert!(!repo.is_bare());
    }

    #[test]
    fn smoke_init_bare() {
        let td = TempDir::new("test").unwrap();
        let path = td.path();

        let repo = Repository::init(path, true).unwrap();
        assert!(repo.is_bare());
    }

    #[test]
    fn smoke_open() {
        let td = TempDir::new("test").unwrap();
        let path = td.path();
        git!(td.path(), "init");

        let repo = Repository::open(path).unwrap();
        assert!(!repo.is_bare());
        assert!(!repo.is_shallow());
        assert!(repo.is_empty().unwrap());
        assert!(repo.path() == td.path().join(".git"));
        assert_eq!(repo.state(), ::Clean);
    }

    #[test]
    fn smoke_open_bare() {
        let td = TempDir::new("test").unwrap();
        let path = td.path();
        git!(td.path(), "init", "--bare");

        let repo = Repository::open(path).unwrap();
        assert!(repo.is_bare());
        assert!(repo.path() == *td.path());
    }

    #[test]
    fn smoke_revparse() {
        let td = TempDir::new("test").unwrap();
        git!(td.path(), "init");
        File::create(&td.path().join("foo")).write_str("foobar").unwrap();
        git!(td.path(), "add", ".");
        git!(td.path(), "commit", "-m", "foo");
        let expected_rev = git!(td.path(), "rev-parse", "HEAD");

        let repo = Repository::open(td.path()).unwrap();
        let actual_rev = repo.revparse("HEAD").unwrap();
        let from = actual_rev.from().unwrap();
        assert!(actual_rev.to().is_none());
        assert_eq!(expected_rev, from.id().to_string());

        assert_eq!(repo.revparse_single("HEAD").unwrap().id().to_string(),
                   expected_rev);
    }
}
